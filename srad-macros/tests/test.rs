use srad_macros::Template;


#[derive(Template)]
struct Test {
    x: i32,
    y: i32,
}

// #[derive(Template)]
// enum AA{
//     A,
//     B
// }