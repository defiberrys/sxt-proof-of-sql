use super::{numerical_util::{divide_columns, modulo_columns}, DynProofExpr, ProofExpr};
use crate::{
    base::database::{try_divide_column_types, Column},
    sql::proof::SumcheckSubpolynomialType,
    utils::log,
};
use serde::{Deserialize, Serialize};

/// Provable numerical `/` expression
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DivisionExpr {
    lhs: Box<DynProofExpr>,
    rhs: Box<DynProofExpr>,
}

impl DivisionExpr {
    /// Create numerical `/` expression
    pub fn new(lhs: Box<DynProofExpr>, rhs: Box<DynProofExpr>) -> Self {
        Self { lhs, rhs }
    }
}

impl ProofExpr for DivisionExpr {
    fn data_type(&self) -> crate::base::database::ColumnType {
        try_divide_column_types(self.lhs.data_type(), self.rhs.data_type())
            .expect("Failed to divide column types")
    }

    fn result_evaluate<'a, S: crate::base::scalar::Scalar>(
        &self,
        alloc: &'a bumpalo::Bump,
        table: &crate::base::database::Table<'a, S>,
    ) -> crate::base::database::Column<'a, S> {
        let lhs_column: Column<'a, S> = self.lhs.result_evaluate(alloc, table);
        let rhs_column: Column<'a, S> = self.rhs.result_evaluate(alloc, table);
        divide_columns(&lhs_column, &rhs_column, alloc)
    }

    fn prover_evaluate<'a, S: crate::base::scalar::Scalar>(
        &self,
        builder: &mut crate::sql::proof::FinalRoundBuilder<'a, S>,
        alloc: &'a bumpalo::Bump,
        table: &crate::base::database::Table<'a, S>,
    ) -> crate::base::database::Column<'a, S> {
        log::log_memory_usage("Start");

        let lhs_column: Column<'a, S> = self.lhs.prover_evaluate(builder, alloc, table);
        let rhs_column: Column<'a, S> = self.rhs.prover_evaluate(builder, alloc, table);

        // lhs_divided_by_rhs
        let lhs_divided_by_rhs = divide_columns(&lhs_column, &rhs_column, alloc);
        let lhs_mod_rhs = modulo_columns(&lhs_column, &rhs_column, alloc);
        builder.produce_intermediate_mle(lhs_divided_by_rhs);
        builder.produce_intermediate_mle(lhs_mod_rhs);

        // subpolynomial: lhs_divided_by_rhs * rhs - lhs + remainder
        builder.produce_sumcheck_subpolynomial(
            SumcheckSubpolynomialType::Identity,
            vec![
                (
                    S::one(),
                    vec![Box::new(lhs_divided_by_rhs), Box::new(rhs_column)],
                ),
                (S::one(), vec![Box::new(lhs_mod_rhs)]),
                (-S::one(), vec![Box::new(lhs_column)]),
            ],
        );

        log::log_memory_usage("End");

        lhs_divided_by_rhs
    }

    fn verifier_evaluate<S: crate::base::scalar::Scalar>(
        &self,
        builder: &mut crate::sql::proof::VerificationBuilder<S>,
        accessor: &crate::base::map::IndexMap<crate::base::database::ColumnRef, S>,
        one_eval: S,
    ) -> Result<S, crate::base::proof::ProofError> {
        let lhs = self.lhs.verifier_evaluate(builder, accessor, one_eval)?;
        let rhs = self.rhs.verifier_evaluate(builder, accessor, one_eval)?;

        // lhs_times_rhs
        let lhs_divided_by_rhs = builder.try_consume_final_round_mle_evaluation()?;
        let lhs_mod_rhs = builder.try_consume_final_round_mle_evaluation()?;

        // subpolynomial: lhs_divided_by_rhs * rhs - lhs + remainder
        builder.try_produce_sumcheck_subpolynomial_evaluation(
            SumcheckSubpolynomialType::Identity,
            lhs_divided_by_rhs * rhs - lhs + lhs_mod_rhs,
            2,
        )?;

        // selection
        Ok(lhs_divided_by_rhs)
    }

    fn get_column_references(
        &self,
        columns: &mut crate::base::map::IndexSet<crate::base::database::ColumnRef>,
    ) {
        self.lhs.get_column_references(columns);
        self.rhs.get_column_references(columns);
    }
}
