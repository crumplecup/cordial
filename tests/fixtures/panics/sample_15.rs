
struct Channel;

impl Channel {
    pub fn demonstrate_delivery(self, value: i32) -> Token {
        self.send(value).unwrap();
        Token
    }

    fn send(&self, _value: i32) -> Result<(), &'static str> {
        Ok(())
    }
}

struct Token;

some_crate::harness! {
    kani, VERIFY_DELIVERY_SRC, {
        #[kani::proof]
        fn verify_delivery() {
            let value: i32 = kani::any();
            let channel = Channel;
            let _token = channel.demonstrate_delivery(value);
        }
    }
}
