use crate::display_driver::display::Screen;

pub struct Console {
    pub screen: Screen,
}


impl Console {
    pub fn new(screen: Screen) -> Self {
        Console { screen }
    }
}