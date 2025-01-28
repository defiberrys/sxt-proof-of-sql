use super::{prover_evaluate_sign, verifier_evaluate_sign};
use crate::{
    base::{
        database::{try_divide_modulo_column_types, Column, ColumnRef, ColumnType, Table},
        map::{IndexMap, IndexSet},
        proof::ProofError,
        scalar::Scalar,
    },
    sql::{
        proof::{FinalRoundBuilder, SumcheckSubpolynomialType, VerificationBuilder},
        proof_exprs::{
            absolute_eval, absolute_value_of_columns, add_subtract_columns,
            columns_to_scalar_slice, divide_columns, inverse_of_columns, modulo_columns,
            multiply_columns, scale_and_add_subtract_eval, sign_column, sign_column_zero_positive,
            DynProofExpr, ProofExpr,
        },
    },
    utils::log,
};
use bumpalo::Bump;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DivideAndModuloExpr {
    pub lhs: Box<DynProofExpr>,
    pub rhs: Box<DynProofExpr>,
}

impl DivideAndModuloExpr {
    fn min_value<S: Scalar>(&self) -> S {
        match self.data_type() {
            ColumnType::Boolean => todo!(),
            ColumnType::Uint8 => todo!(),
            ColumnType::TinyInt => todo!(),
            ColumnType::SmallInt => todo!(),
            ColumnType::Int => todo!(),
            ColumnType::BigInt => todo!(),
            ColumnType::Int128 => todo!(),
            ColumnType::VarChar => todo!(),
            ColumnType::Decimal75(precision, _) => todo!(),
            ColumnType::TimestampTZ(po_sqltime_unit, po_sqltime_zone) => todo!(),
            ColumnType::Scalar => todo!(),
        }
    }

    pub fn new(lhs: Box<DynProofExpr>, rhs: Box<DynProofExpr>) -> Self {
        Self { lhs, rhs }
    }

    pub fn data_type(&self) -> ColumnType {
        try_divide_modulo_column_types(self.lhs.data_type(), self.rhs.data_type())
            .expect("Failed to divide/modulo column types")
    }

    pub fn prover_evaluate<'a, S: Scalar>(
        &self,
        builder: &mut FinalRoundBuilder<'a, S>,
        alloc: &'a Bump,
        table: &Table<'a, S>,
    ) -> (Column<'a, S>, Column<'a, S>) {
        log::log_memory_usage("Start");

        let lhs_column: Column<'a, S> = self.lhs.prover_evaluate(builder, alloc, table);
        let rhs_column: Column<'a, S> = self.rhs.prover_evaluate(builder, alloc, table);

        // lhs_divided_by_rhs
        let quotient = divide_columns(&lhs_column, &rhs_column, alloc);
        let remainder = modulo_columns(&lhs_column, &rhs_column, alloc);
        builder.produce_intermediate_mle(quotient);
        builder.produce_intermediate_mle(remainder);

        // subpolynomial: lhs_divided_by_rhs * rhs - lhs + remainder
        builder.produce_sumcheck_subpolynomial(
            SumcheckSubpolynomialType::Identity,
            vec![
                (S::one(), vec![Box::new(quotient), Box::new(rhs_column)]),
                (S::one(), vec![Box::new(remainder)]),
                (-S::one(), vec![Box::new(lhs_column)]),
            ],
        );

        // sign(a) * r = sign(r) * r
        // constrains remainder to be on the correct side of 0.
        let lhs_sign =
            prover_evaluate_sign(builder, alloc, columns_to_scalar_slice(&lhs_column, alloc));
        let remainder_sign =
            prover_evaluate_sign(builder, alloc, columns_to_scalar_slice(&remainder, alloc));

        builder.produce_sumcheck_subpolynomial(
            SumcheckSubpolynomialType::Identity,
            vec![
                (S::one(), vec![Box::new(lhs_sign), Box::new(remainder)]),
                (
                    -S::one(),
                    vec![Box::new(remainder_sign), Box::new(remainder)],
                ),
            ],
        );

        // sign(|r| - |b|) * b = 0, which is the same as
        // sign((2 * sign(r) - 1) * r - (2 * sign(b) - 1) * b) * b = 0
        // constrains absolute value of remainder to be within absolute value of rhs
        let rhs_sign =
            prover_evaluate_sign(builder, alloc, columns_to_scalar_slice(&rhs_column, alloc));
        let absolute_value_remainder = absolute_value_of_columns(remainder_sign, &remainder, alloc);
        let absolute_value_rhs = absolute_value_of_columns(rhs_sign, &rhs_column, alloc);
        let remainder_and_rhs_difference_sign = prover_evaluate_sign(
            builder,
            alloc,
            add_subtract_columns(
                absolute_value_remainder,
                absolute_value_rhs,
                0,
                0,
                alloc,
                true,
            ),
        );

        builder.produce_sumcheck_subpolynomial(
            SumcheckSubpolynomialType::Identity,
            vec![(
                S::one(),
                vec![
                    Box::new(remainder_and_rhs_difference_sign),
                    Box::new(rhs_column),
                ],
            )],
        );

        // q * b * b^-1 = q
        // This ensures that q = 0 if b = 0
        let rhs_inverse = inverse_of_columns(rhs_column, alloc);
        builder.produce_intermediate_mle(rhs_inverse);

        builder.produce_sumcheck_subpolynomial(
            SumcheckSubpolynomialType::Identity,
            vec![
                (
                    S::one(),
                    vec![
                        Box::new(quotient),
                        Box::new(rhs_column),
                        Box::new(rhs_inverse),
                    ],
                ),
                (-S::one(), vec![Box::new(quotient)]),
            ],
        );

        log::log_memory_usage("End");

        (quotient, remainder)
    }

    pub fn verifier_evaluate<S: Scalar>(
        &self,
        builder: &mut VerificationBuilder<S>,
        accessor: &IndexMap<ColumnRef, S>,
        one_eval: S,
    ) -> Result<(S, S), ProofError> {
        let lhs = self.lhs.verifier_evaluate(builder, accessor, one_eval)?;
        let rhs = self.rhs.verifier_evaluate(builder, accessor, one_eval)?;

        // lhs_times_rhs
        let quotient = builder.try_consume_final_round_mle_evaluation()?;
        let remainder = builder.try_consume_final_round_mle_evaluation()?;

        // subpolynomial: lhs_divided_by_rhs * rhs - lhs + remainder
        builder.try_produce_sumcheck_subpolynomial_evaluation(
            SumcheckSubpolynomialType::Identity,
            quotient * rhs - lhs + remainder,
            2,
        )?;

        // sign(a) * r = sign(r) * r
        let lhs_sign = verifier_evaluate_sign(builder, lhs, one_eval)?;
        let remainder_sign = verifier_evaluate_sign(builder, remainder, one_eval)?;

        builder.try_produce_sumcheck_subpolynomial_evaluation(
            SumcheckSubpolynomialType::Identity,
            remainder_sign * remainder - lhs_sign * lhs,
            2,
        )?;

        // sign((2 * sign(r) - 1) * r - (2 * sign(b) - 1) * b) * b = 0
        let rhs_sign = verifier_evaluate_sign(builder, rhs, one_eval)?;
        let remainder_and_rhs_difference_sign = verifier_evaluate_sign(
            builder,
            scale_and_add_subtract_eval(
                absolute_eval(remainder_sign, remainder),
                absolute_eval(rhs_sign, rhs),
                0,
                0,
                true,
            ),
            one_eval,
        )?;

        builder.try_produce_sumcheck_subpolynomial_evaluation(
            SumcheckSubpolynomialType::Identity,
            remainder_and_rhs_difference_sign * rhs,
            2,
        )?;

        // q * b * b^-1 = q
        let rhs_inverse = builder.try_consume_final_round_mle_evaluation()?;

        builder.try_produce_sumcheck_subpolynomial_evaluation(
            SumcheckSubpolynomialType::Identity,
            quotient * (S::ONE - rhs * rhs_inverse),
            3,
        )?;

        // selection
        Ok((quotient, remainder))
    }

    pub fn get_column_references(&self, columns: &mut IndexSet<ColumnRef>) {
        self.lhs.get_column_references(columns);
        self.rhs.get_column_references(columns);
    }
}
