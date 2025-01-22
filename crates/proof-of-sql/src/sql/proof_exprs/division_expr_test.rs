use crate::{
    base::{
        commitment::InnerProductProof,
        database::{
            owned_table_utility::*, OwnedTableTestAccessor,
        },
    },
    sql::{
        proof::{exercise_verification, VerifiableQueryResult},
        proof_exprs::test_utility::*,
        proof_plans::test_utility::*,
    },
};

// select a * 2 as a, c, b * 4.5 as b, d * 3  + 4.7 as d, e from sxt.t where d * 3.9 = 8.19
#[test]
fn we_can_prove_a_typical_divide_query() {
    let data = owned_table([
        smallint("a", [1_i16, 2, 3, 4]),
        int("b", [0_i32, 1, 47, 1]),
        varchar("e", ["ab", "t", "efg", "g"]),
        bigint("c", [0_i64, 2, 2, 0]),
        decimal75("d", 2, 1, [21_i64, 4, 21, -7]),
    ]);
    let t = "sxt.t".parse().unwrap();
    let accessor = OwnedTableTestAccessor::<InnerProductProof>::new_from_table(t, data, 0, ());
    let ast = filter(
        vec![
            aliased_plan(divide(column(t, "a", &accessor), const_int(2)), "a"),
            col_expr_plan(t, "c", &accessor),
            aliased_plan(
                divide(column(t, "b", &accessor), const_decimal75(2, 1, 45)),
                "b",
            ),
            aliased_plan(
                add(
                    divide(column(t, "d", &accessor), const_smallint(3)),
                    const_decimal75(2, 1, 47),
                ),
                "d",
            ),
            col_expr_plan(t, "e", &accessor),
        ],
        tab(t),
        equal(
            multiply(column(t, "d", &accessor), const_decimal75(2, 1, 39)),
            const_decimal75(3, 2, 819),
        ),
    );
    let verifiable_res = VerifiableQueryResult::new(&ast, &accessor, &());
    exercise_verification(&verifiable_res, &ast, &accessor, t);
    let res = verifiable_res.verify(&ast, &accessor, &()).unwrap().table;
    let expected_res = owned_table([
        int("a", [0_i32, 1]),
        bigint("c", [0_i64, 2]),
        decimal75("b", 13, 1, [0_i64, 1]),
        decimal75("d", 9, 1, [54_i64, 54]),
        varchar("e", ["ab", "efg"]),
    ]);
    assert_eq!(res, expected_res);
}