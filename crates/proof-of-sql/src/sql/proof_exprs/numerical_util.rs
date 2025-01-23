use crate::base::{
    database::{Column, ColumnarValue, LiteralValue},
    scalar::{Scalar, ScalarExt},
};
use bumpalo::Bump;
use core::cmp::Ordering;
use num_traits::{Num, NumCast};

#[allow(clippy::cast_sign_loss)]
/// Add or subtract two literals together.
pub(crate) fn add_subtract_literals<S: Scalar>(
    lhs: &LiteralValue,
    rhs: &LiteralValue,
    lhs_scale: i8,
    rhs_scale: i8,
    is_subtract: bool,
) -> S {
    let (lhs_scaled, rhs_scaled) = match lhs_scale.cmp(&rhs_scale) {
        Ordering::Less => {
            let scaling_factor = S::pow10((rhs_scale - lhs_scale) as u8);
            (lhs.to_scalar::<S>() * scaling_factor, rhs.to_scalar())
        }
        Ordering::Equal => (lhs.to_scalar(), rhs.to_scalar()),
        Ordering::Greater => {
            let scaling_factor = S::pow10((lhs_scale - rhs_scale) as u8);
            (lhs.to_scalar(), rhs.to_scalar::<S>() * scaling_factor)
        }
    };
    if is_subtract {
        lhs_scaled - rhs_scaled
    } else {
        lhs_scaled + rhs_scaled
    }
}

#[allow(
    clippy::missing_panics_doc,
    reason = "lhs and rhs are guaranteed to have the same length by design, ensuring no panic occurs"
)]
/// Add or subtract two columns together.
pub(crate) fn add_subtract_columns<'a, S: Scalar>(
    lhs: Column<'a, S>,
    rhs: Column<'a, S>,
    lhs_scale: i8,
    rhs_scale: i8,
    alloc: &'a Bump,
    is_subtract: bool,
) -> &'a [S] {
    let lhs_len = lhs.len();
    let rhs_len = rhs.len();
    assert!(
        lhs_len == rhs_len,
        "lhs and rhs should have the same length"
    );
    let max_scale = lhs_scale.max(rhs_scale);
    let lhs_scalar = lhs.to_scalar_with_scaling(max_scale - lhs_scale);
    let rhs_scalar = rhs.to_scalar_with_scaling(max_scale - rhs_scale);
    let result = alloc.alloc_slice_fill_with(lhs_len, |i| {
        if is_subtract {
            lhs_scalar[i] - rhs_scalar[i]
        } else {
            lhs_scalar[i] + rhs_scalar[i]
        }
    });
    result
}

/// Add or subtract two [`ColumnarValues`] together.
#[allow(dead_code)]
pub(crate) fn add_subtract_columnar_values<'a, S: Scalar>(
    lhs: ColumnarValue<'a, S>,
    rhs: ColumnarValue<'a, S>,
    lhs_scale: i8,
    rhs_scale: i8,
    alloc: &'a Bump,
    is_subtract: bool,
) -> ColumnarValue<'a, S> {
    match (lhs, rhs) {
        (ColumnarValue::Column(lhs), ColumnarValue::Column(rhs)) => {
            ColumnarValue::Column(Column::Scalar(add_subtract_columns(
                lhs,
                rhs,
                lhs_scale,
                rhs_scale,
                alloc,
                is_subtract,
            )))
        }
        (ColumnarValue::Literal(lhs), ColumnarValue::Column(rhs)) => {
            ColumnarValue::Column(Column::Scalar(add_subtract_columns(
                Column::from_literal_with_length(&lhs, rhs.len(), alloc),
                rhs,
                lhs_scale,
                rhs_scale,
                alloc,
                is_subtract,
            )))
        }
        (ColumnarValue::Column(lhs), ColumnarValue::Literal(rhs)) => {
            ColumnarValue::Column(Column::Scalar(add_subtract_columns(
                lhs,
                Column::from_literal_with_length(&rhs, lhs.len(), alloc),
                lhs_scale,
                rhs_scale,
                alloc,
                is_subtract,
            )))
        }
        (ColumnarValue::Literal(lhs), ColumnarValue::Literal(rhs)) => {
            ColumnarValue::Literal(LiteralValue::Scalar(
                add_subtract_literals::<S>(&lhs, &rhs, lhs_scale, rhs_scale, is_subtract).into(),
            ))
        }
    }
}

/// Multiply two columns together.
/// # Panics
/// Panics if: `lhs` and `rhs` are not of the same length.
pub(crate) fn multiply_columns<'a, S: Scalar>(
    lhs: &Column<'a, S>,
    rhs: &Column<'a, S>,
    alloc: &'a Bump,
) -> &'a [S] {
    let lhs_len = lhs.len();
    let rhs_len = rhs.len();
    assert!(
        lhs_len == rhs_len,
        "lhs and rhs should have the same length"
    );
    alloc.alloc_slice_fill_with(lhs_len, |i| {
        lhs.scalar_at(i).unwrap() * rhs.scalar_at(i).unwrap()
    })
}

#[allow(dead_code)]
/// Multiply two [`ColumnarValues`] together.
/// # Panics
/// Panics if: `lhs` and `rhs` are not of the same length.
pub(crate) fn multiply_columnar_values<'a, S: Scalar>(
    lhs: &ColumnarValue<'a, S>,
    rhs: &ColumnarValue<'a, S>,
    alloc: &'a Bump,
) -> ColumnarValue<'a, S> {
    match (lhs, rhs) {
        (ColumnarValue::Column(lhs), ColumnarValue::Column(rhs)) => {
            ColumnarValue::Column(Column::Scalar(multiply_columns(lhs, rhs, alloc)))
        }
        (ColumnarValue::Literal(lhs), ColumnarValue::Column(rhs)) => {
            let lhs_scalar = lhs.to_scalar::<S>();
            let result =
                alloc.alloc_slice_fill_with(rhs.len(), |i| lhs_scalar * rhs.scalar_at(i).unwrap());
            ColumnarValue::Column(Column::Scalar(result))
        }
        (ColumnarValue::Column(lhs), ColumnarValue::Literal(rhs)) => {
            let rhs_scalar = rhs.to_scalar();
            let result =
                alloc.alloc_slice_fill_with(lhs.len(), |i| lhs.scalar_at(i).unwrap() * rhs_scalar);
            ColumnarValue::Column(Column::Scalar(result))
        }
        (ColumnarValue::Literal(lhs), ColumnarValue::Literal(rhs)) => {
            let result = lhs.to_scalar::<S>() * rhs.to_scalar();
            ColumnarValue::Literal(LiteralValue::Scalar(result.into()))
        }
    }
}

#[allow(
    clippy::missing_panics_doc,
    reason = "scaling factor is guaranteed to not be negative based on input validation prior to calling this function"
)]
/// The counterpart of `add_subtract_columns` for evaluating decimal expressions.
pub(crate) fn scale_and_add_subtract_eval<S: Scalar>(
    lhs_eval: S,
    rhs_eval: S,
    lhs_scale: i8,
    rhs_scale: i8,
    is_subtract: bool,
) -> S {
    let max_scale = lhs_scale.max(rhs_scale);
    let left_scaled_eval = lhs_eval * S::pow10(max_scale.abs_diff(lhs_scale));
    let right_scaled_eval = rhs_eval * S::pow10(max_scale.abs_diff(rhs_scale));
    if is_subtract {
        left_scaled_eval - right_scaled_eval
    } else {
        left_scaled_eval + right_scaled_eval
    }
}

fn divide_integer_columns<
    'a,
    L: NumCast + Default + Num + Copy,
    R: NumCast + Default + Num + Copy,
>(
    lhs: &&[L],
    rhs: &&[R],
    alloc: &'a Bump,
    is_right_bigger_int_type: bool,
) -> &'a [L] {
    let division = alloc.alloc_slice_fill_with(lhs.len(), |_| L::default());
    division
        .iter_mut()
        .zip(lhs.iter().zip(rhs.iter()))
        .for_each(|(d, (l, r))| {
            *d = if is_right_bigger_int_type {
                let l_cast: R = NumCast::from(*l).unwrap();
                NumCast::from(l_cast / *r).unwrap()
            } else {
                let r_cast: L = NumCast::from(*r).unwrap();
                *l / r_cast
            }
        });
    division
}

fn modulo_integer_columns<
    'a,
    L: NumCast + Default + Num + Copy,
    R: NumCast + Default + Num + Copy,
>(
    lhs: &&[L],
    rhs: &&[R],
    alloc: &'a Bump,
    is_right_bigger_int_type: bool,
) -> &'a [R] {
    let remainder = alloc.alloc_slice_fill_with(lhs.len(), |_| R::default());
    remainder
        .iter_mut()
        .zip(lhs.iter().zip(rhs.iter()))
        .for_each(|(m, (l, r))| {
            *m = if is_right_bigger_int_type {
                let l_cast: R = NumCast::from(*l).unwrap();
                l_cast % *r
            } else {
                let r_cast: L = NumCast::from(*r).unwrap();
                NumCast::from(*l % r_cast).unwrap()
            }
        });
    remainder
}

/// Divide one column by another.
/// # Panics
/// Panics if: `lhs` and `rhs` are not of the same length.
pub(crate) fn divide_columns<'a, S: Scalar>(
    lhs: &Column<'a, S>,
    rhs: &Column<'a, S>,
    alloc: &'a Bump,
) -> Column<'a, S> {
    let lhs_len = lhs.len();
    let rhs_len = rhs.len();
    assert!(
        lhs_len == rhs_len,
        "lhs and rhs should have the same length"
    );
    match (lhs, rhs) {
        (Column::Int128(left), Column::Int128(right)) => {
            Column::Int128(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::BigInt(right)) => {
            Column::Int128(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::Int(right)) => {
            Column::Int128(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::SmallInt(right)) => {
            Column::Int128(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::TinyInt(right)) => {
            Column::Int128(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::Uint8(right)) => {
            Column::Int128(divide_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::Int128(right)) => {
            Column::BigInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::BigInt(left), Column::BigInt(right)) => {
            Column::BigInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::Int(right)) => {
            Column::BigInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::SmallInt(right)) => {
            Column::BigInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::TinyInt(right)) => {
            Column::BigInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::Uint8(right)) => {
            Column::BigInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::Int128(right)) => {
            Column::Int(divide_integer_columns(left, right, alloc, true))
        }
        (Column::Int(left), Column::BigInt(right)) => {
            Column::Int(divide_integer_columns(left, right, alloc, true))
        }
        (Column::Int(left), Column::Int(right)) => {
            Column::Int(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::SmallInt(right)) => {
            Column::Int(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::TinyInt(right)) => {
            Column::Int(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::Uint8(right)) => {
            Column::Int(divide_integer_columns(left, right, alloc, false))
        }
        (Column::SmallInt(left), Column::Int128(right)) => {
            Column::SmallInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::SmallInt(left), Column::BigInt(right)) => {
            Column::SmallInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::SmallInt(left), Column::Int(right)) => {
            Column::SmallInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::SmallInt(left), Column::SmallInt(right)) => {
            Column::SmallInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::SmallInt(left), Column::TinyInt(right)) => {
            Column::SmallInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::SmallInt(left), Column::Uint8(right)) => {
            Column::SmallInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::TinyInt(left), Column::Int128(right)) => {
            Column::TinyInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::BigInt(right)) => {
            Column::TinyInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::Int(right)) => {
            Column::TinyInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::SmallInt(right)) => {
            Column::TinyInt(divide_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::TinyInt(right)) => {
            Column::TinyInt(divide_integer_columns(left, right, alloc, false))
        }
        (Column::Uint8(left), Column::Uint8(right)) => {
            Column::Uint8(divide_integer_columns(left, right, alloc, false))
        }
        (Column::TinyInt(left), Column::Uint8(right)) => Column::TinyInt(divide_integer_columns(
            left,
            &&right
                .iter()
                .map(|&x| x as i16)
                .collect::<Vec<_>>()
                .as_slice(),
            alloc,
            true,
        )),
        _ => todo!(),
    }
}

/// Take the modulo of one column against another.
/// # Panics
/// Panics if: `lhs` and `rhs` are not of the same length.
pub(crate) fn modulo_columns<'a, S: Scalar>(
    lhs: &Column<'a, S>,
    rhs: &Column<'a, S>,
    alloc: &'a Bump,
) -> Column<'a, S> {
    let lhs_len = lhs.len();
    let rhs_len = rhs.len();
    assert!(
        lhs_len == rhs_len,
        "lhs and rhs should have the same length"
    );
    match (lhs, rhs) {
        (Column::Int128(left), Column::Int128(right)) => {
            Column::Int128(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::BigInt(right)) => {
            Column::BigInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::Int(right)) => {
            Column::Int(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::SmallInt(right)) => {
            Column::SmallInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::TinyInt(right)) => {
            Column::TinyInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int128(left), Column::Uint8(right)) => {
            Column::Uint8(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::Int128(right)) => {
            Column::Int128(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::BigInt(left), Column::BigInt(right)) => {
            Column::BigInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::Int(right)) => {
            Column::Int(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::SmallInt(right)) => {
            Column::SmallInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::TinyInt(right)) => {
            Column::TinyInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::BigInt(left), Column::Uint8(right)) => {
            Column::Uint8(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::Int128(right)) => {
            Column::Int128(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::Int(left), Column::BigInt(right)) => {
            Column::BigInt(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::Int(left), Column::Int(right)) => {
            Column::Int(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::SmallInt(right)) => {
            Column::SmallInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::TinyInt(right)) => {
            Column::TinyInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Int(left), Column::Uint8(right)) => {
            Column::Uint8(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::SmallInt(left), Column::Int128(right)) => {
            Column::Int128(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::SmallInt(left), Column::BigInt(right)) => {
            Column::BigInt(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::SmallInt(left), Column::Int(right)) => {
            Column::Int(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::SmallInt(left), Column::SmallInt(right)) => {
            Column::SmallInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::SmallInt(left), Column::TinyInt(right)) => {
            Column::TinyInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::SmallInt(left), Column::Uint8(right)) => {
            Column::Uint8(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::TinyInt(left), Column::Int128(right)) => {
            Column::Int128(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::BigInt(right)) => {
            Column::BigInt(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::Int(right)) => {
            Column::Int(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::SmallInt(right)) => {
            Column::SmallInt(modulo_integer_columns(left, right, alloc, true))
        }
        (Column::TinyInt(left), Column::TinyInt(right)) => {
            Column::TinyInt(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::Uint8(left), Column::Uint8(right)) => {
            Column::Uint8(modulo_integer_columns(left, right, alloc, false))
        }
        (Column::TinyInt(left), Column::Uint8(right)) => Column::Uint8(modulo_integer_columns(
            &left
                .iter()
                .map(|&x| x as i16)
                .collect::<Vec<_>>()
                .as_slice(),
            right,
            alloc,
            false,
        )),
        _ => todo!(),
    }
}
