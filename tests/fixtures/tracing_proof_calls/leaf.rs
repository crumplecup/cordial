pub fn leaf() {}
pub fn bridge() { leaf(); }
pub fn mixed() {}
pub fn runtime() { mixed(); }
pub fn assertion_target() {}
pub fn message_only() {}
pub fn opaque_only() {}
pub struct Widget<T>(T);
impl Widget<u8> { pub fn ensures() {} }
#[kani::proof]
impl Widget<u16> { pub fn seeded() {} }
pub trait Contract {
    fn check();
    fn defaulted() { opaque_only(); }
}
impl Contract for Widget<u32> { fn check() {} }
