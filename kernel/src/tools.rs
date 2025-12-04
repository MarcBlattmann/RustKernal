pub fn digit_count(mut number: u64) -> usize {
    if number == 0 {
        return 1;
    }

    let mut digits = 0;

    while number > 0 {
        digits += 1;
        number /= 10;
    }

    digits
}