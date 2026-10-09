// SPDX-License-Identifier: MIT OR Apache-2.0
use cw32_data_macros::EnumDebug;

#[derive(EnumDebug)]
enum Example {
    Ready,
    Number(u8),
    Nested(Vec<u8>),
}

#[derive(EnumDebug)]
enum Generic<'a, T, const N: usize>
where
    T: Copy,
{
    Borrowed(&'a T),
    Array([T; N]),
}

#[derive(EnumDebug)]
enum Never {}

#[test]
fn names_are_qualified_and_payloads_use_debug() {
    assert_eq!(format!("{:?}", Example::Ready), "Example::Ready");
    assert_eq!(format!("{:?}", Example::Number(7)), "Example::Number(7)");
    assert_eq!(
        format!("{:?}", Example::Nested(vec![1, 2])),
        "Example::Nested([1, 2])"
    );
    assert_eq!(
        format!("{:#?}", Example::Number(7)),
        "Example::Number(\n    7,\n)"
    );
}

#[test]
fn generics_lifetimes_constraints_and_empty_enums_compile() {
    assert_eq!(
        format!("{:?}", Generic::<'_, u8, 2>::Borrowed(&5)),
        "Generic::Borrowed(5)"
    );
    assert_eq!(
        format!("{:?}", Generic::<'_, u8, 2>::Array([2, 3])),
        "Generic::Array([2, 3])"
    );
    fn require_debug<T: core::fmt::Debug>() {}
    require_debug::<Never>();
}

#[derive(EnumDebug)]
enum Recursive {
    End,
    Next(Box<Recursive>),
}

#[test]
fn recursive_payloads_do_not_create_cyclic_debug_bounds() {
    assert_eq!(
        format!("{:?}", Recursive::Next(Box::new(Recursive::End))),
        "Recursive::Next(Recursive::End)"
    );
}
