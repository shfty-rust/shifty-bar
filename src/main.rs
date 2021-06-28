use chrono::prelude::*;

const ALIGN_LEFT: &str = "%{l}";
const ALIGN_CENTER: &str = "%{c}";
const ALIGN_RIGHT: &str = "%{r}";

const LEFT_SEPARATOR: &str = "";
const RIGHT_SEPARATOR: &str = "";

fn foreground_reset() -> &'static str {
    "%{F-}"
}

fn background_reset() -> &'static str {
    "%{B-}"
}

fn foreground<const HEX: usize>() -> String {
    format!("%{{F#{:08X}}}", HEX)
}

fn background<const HEX: usize>() -> String {
    format!("%{{B#{:08X}}}", HEX)
}

fn padding<const COUNT: usize>() -> String {
    std::iter::repeat(" ").take(COUNT).collect()
}

fn main() {
    loop {
        println!(
            "{}{}{}{}{}{}",
            ALIGN_LEFT,
            left_bar(),
            ALIGN_CENTER,
            center_bar(),
            ALIGN_RIGHT,
            right_bar(),
        );
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}

fn left_bar() -> String {
    widget_hostname()
}

fn center_bar() -> String {
    Default::default()
}

fn right_bar() -> String {
    widget_clock()
}

fn widget_hostname() -> String {
    let hostname = hostname::get()
        .expect("Failed to get hostname")
        .into_string()
        .expect("Failed to convert hostname to String");

    IntoIterator::into_iter([
        &foreground::<0xFF000000>(),
        &background::<0xFFFFFFFF>(),
        " ",
        &hostname,
        " ",
        &foreground::<0xFFFFFFFF>(),
        &background::<0x00000000>(),
        LEFT_SEPARATOR,
        foreground_reset(),
        background_reset(),
    ])
    .collect()
}

fn widget_clock() -> String {
    let local: DateTime<Local> = Local::now();
    let naive = local.naive_local();
    IntoIterator::into_iter([
        &foreground::<0xFF000000>(),
        background_reset(),
        &RIGHT_SEPARATOR,
        &foreground::<0xFFFFFFFF>(),
        &background::<0xFF000000>(),
        &padding::<1>(),
        &local.format("%D %H:%M:%S %p").to_string(),
        "  ",
        foreground_reset(),
        background_reset(),
    ])
    .collect()
}
