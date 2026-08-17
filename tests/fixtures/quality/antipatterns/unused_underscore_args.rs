trait Agent {
    fn handle(&self, value: i32) -> bool;
}

struct Bot;

impl Agent for Bot {
    fn handle(&self, _value: i32) -> bool {
        true
    }
}

fn free_fn(_x: i32, y: i32) {}

fn ignored(_: String) {}

struct Service;

impl Service {
    fn method(&self, _ctx: &str) {}
}

trait Declared {
    fn placeholder(_input: u32);
}

struct Uses;

impl Uses {
    fn tuple(_a: i32, (b, _c): (i32, i32)) {
        let _ = b;
    }
}

#[cfg(test)]
mod tests {
    fn test_fn(_unused: i32) {}
}
