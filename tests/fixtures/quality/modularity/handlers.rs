pub fn small_helper() {
    let _ = 1;
}

pub fn medium_handler() {
    let mut total = 0;
    for offset in 0..12 {
        total += offset;
    }
    let _ = total;
}

pub fn large_handler() {
    let mut acc = 0;
    acc += step(1);
    acc += step(2);
    acc += step(3);
    acc += step(4);
    acc += step(5);
    acc += step(6);
    acc += step(7);
    acc += step(8);
    acc += step(9);
    acc += step(10);
    acc += step(11);
    acc += step(12);
    acc += step(13);
    acc += step(14);
    acc += step(15);
    acc += step(16);
    acc += step(17);
    acc += step(18);
    acc += step(19);
    acc += step(20);
    acc += step(21);
    acc += step(22);
    acc += step(23);
    acc += step(24);
    acc += step(25);
    acc += step(26);
    acc += step(27);
    acc += step(28);
    acc += step(29);
    acc += step(30);
    acc += step(31);
    acc += step(32);
    acc += step(33);
    acc += step(34);
    acc += step(35);
    acc += step(36);
    acc += step(37);
    acc += step(38);
    acc += step(39);
    acc += step(40);
    let _ = acc;
}

fn step(value: i32) -> i32 {
    value + 1
}

#[cfg(test)]
mod tests {
    fn ignored_large() {
        let _ = (1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
    }
}
