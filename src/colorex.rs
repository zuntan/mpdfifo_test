use std::fmt;
use termion::color;

macro_rules! csi_a {
    ($( $l:expr ),*) => { concat!("\x1B[", $( $l ),*) };
}

macro_rules! derive_color_a {
    ($doc:expr, $name:ident, $value:expr) => {
        #[doc = $doc]
        #[derive(Copy, Clone, Debug)]
        pub struct $name;

        impl color::Color for $name {
            #[inline]
            fn write_fg(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(self.fg_str())
            }

            #[inline]
            fn write_bg(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str(self.bg_str())
            }
        }

        impl $name {
            #[inline]
            /// Returns the ANSI escape sequence as a string.
            pub fn fg_str(&self) -> &'static str { csi_a!("3", $value, "m") }

            #[inline]
            /// Returns the ANSI escape sequences as a string.
            pub fn bg_str(&self) -> &'static str { csi_a!("4", $value, "m") }
        }
    };
}

derive_color_a!("Black. Ansi", Black, "0");
derive_color_a!("Red. Ansi", Red, "1");
derive_color_a!("Green. Ansi", Green, "2");
derive_color_a!("Yellow. Ansi", Yellow, "3");
derive_color_a!("Blue. Ansi", Blue, "4");
derive_color_a!("Magenta. Ansi", Magenta, "5");
derive_color_a!("Cyan. Ansi", Cyan, "6");
derive_color_a!("White. Ansi", White, "7");
