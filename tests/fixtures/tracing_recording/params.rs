use std::fmt::Debug;

#[derive(Debug)]
pub struct Shown(u32);

pub struct Hidden(u32);

pub fn derived(value: Shown) -> u32 { value.0 }

pub fn collection(values: Vec<Shown>) -> usize { values.len() }

pub fn bounded<T: Debug>(value: T) -> String { format!("{value:?}") }

pub fn unbounded<T>(value: T) -> usize { let _ = value; 0 }

pub fn opaque(value: Hidden) -> u32 { value.0 }

pub fn destructured((left, right): (Shown, Hidden)) -> u32 { left.0 + right.0 }

pub fn mixed(shown: Shown, hidden: Hidden, count: u32) -> u32 { shown.0 + hidden.0 + count }

pub fn borrowed_path(root: &std::path::Path, name: &str, flag: bool) -> usize { root.len() + name.len() + usize::from(flag) }
