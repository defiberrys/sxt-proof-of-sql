use crate::{
    base::{
        commitment::InnerProductProof,
        database::{owned_table_utility::*, OwnedTableTestAccessor},
    },
    sql::{
        proof::{exercise_verification, VerifiableQueryResult},
        proof_exprs::test_utility::*,
        proof_plans::test_utility::*,
    },
};

// select a / 2 as a from sxt.t where d * 3.9 = 8.19
#[test]
fn we_can_prove_a_typical_divide_query() {
    let data = owned_table([
        smallint("a", [1_i16, 2, 3, 4]),
        decimal75("d", 2, 1, [21_i64, 4, 21, -7]),
    ]);
    let t = "sxt.t".parse().unwrap();
    let accessor = OwnedTableTestAccessor::<InnerProductProof>::new_from_table(t, data, 0, ());
    let ast = filter(
        vec![aliased_plan(
            divide(column(t, "a", &accessor), const_int(2)),
            "a",
        )],
        tab(t),
        equal(
            multiply(column(t, "d", &accessor), const_decimal75(2, 1, 39)),
            const_decimal75(3, 2, 819),
        ),
    );
    let verifiable_res = VerifiableQueryResult::new(&ast, &accessor, &());
    exercise_verification(&verifiable_res, &ast, &accessor, t);
    let res = verifiable_res.verify(&ast, &accessor, &()).unwrap().table;
    let expected_res = owned_table([smallint("a", [0_i16, 1])]);
    assert_eq!(res, expected_res);
}
