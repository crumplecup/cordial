#[derive(Debug)]
pub struct Shown(u32);

pub type Alias = Shown;

impl Shown {
    pub fn from_seed(seed: u32) -> Self { Shown(seed) }

    pub fn from_alias(seed: u32) -> Alias { Shown(seed) }

    pub fn from_checked(seed: u32) -> Option<Self> { Some(Shown(seed)) }
}

pub fn is_ready() -> bool { true }

pub fn has_flag() -> Option<bool> { Some(true) }

pub fn is_total(count: u32) -> u32 { count }
