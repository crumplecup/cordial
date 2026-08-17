struct Widget {
    name: String,
    count: u32,
}

impl Widget {
    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn count(&self) -> u32 {
        self.count
    }
}
