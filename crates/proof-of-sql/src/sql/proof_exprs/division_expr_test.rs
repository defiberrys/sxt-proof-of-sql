use core::i128;

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
        tinyint("a", [1_i8, 2, 3, 4]),
        decimal75("d", 2, 1, [21_i64, 4, 21, -7]),
    ]);
    let t = "sxt.t".parse().unwrap();
    let accessor = OwnedTableTestAccessor::<InnerProductProof>::new_from_table(t, data, 0, ());
    let ast = filter(
        vec![aliased_plan(
            divide(column(t, "a", &accessor), const_uint8(2)),
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
    let expected_res = owned_table([tinyint("a", [0_i8, 1])]);
    assert_eq!(res, expected_res);
}

#[test]
fn we_can_prove_int_division_query() {
    let data = owned_table([
        tinyint("b", [2_i8, -115, 6, 126]),
        smallint("c", [7_i16, 36, -30000, 31104]),
        int("d", [4_32, -115, i32::MIN + 12, 52]),
        bigint("e", [i64::MIN + 366, -68, i64::MAX, 126]),
        int128("f", [6_i128, i128::MIN + 3, 99, i128::MAX - 6]),
    ]);
    let t = "sxt.t".parse().unwrap();
    let accessor = OwnedTableTestAccessor::<InnerProductProof>::new_from_table(t, data, 0, ());
    
    let ast = projection(
        vec![
        aliased_plan(
            divide(column(t, "b", &accessor), const_tinyint(2)),
            "b2",
        ), 
        aliased_plan(
            divide(column(t, "b", &accessor), column(t, "c", &accessor)),
            "bc",
        ), 
        aliased_plan(
            divide(column(t, "b", &accessor), column(t, "d", &accessor)),
            "bd",
        ), 
        aliased_plan(
            divide(column(t, "b", &accessor), column(t, "e", &accessor)),
            "be",
        ), 
        aliased_plan(
            divide(column(t, "b", &accessor), column(t, "f", &accessor)),
            "bf",
        )],
        tab(t),
    );
    let verifiable_res = VerifiableQueryResult::new(&ast, &accessor, &());
    exercise_verification(&verifiable_res, &ast, &accessor, t);
    let res = verifiable_res.verify(&ast, &accessor, &()).unwrap().table;
    let expected_res = owned_table([
        tinyint("b2", [1_i8, -57, 3, 63]),
        tinyint("bc", [0_i8, -3, 0, 0]),
        tinyint("bd", [0_i8, 1, 0, 2]),
        tinyint("be", [0_i8, 1, 0, 1]),
        tinyint("bf", [0_i8, 0, 0, 0])
    ]);
    assert_eq!(res, expected_res);
}
