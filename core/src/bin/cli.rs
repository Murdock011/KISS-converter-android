//! Small command line tool to test the core on a computer.
//! Usage: convert-cli <input> <output> [quality] [max_dimension]
//!        convert-cli --pdf <out.pdf> <img1> <img2> ...

use convert_core::{convert, images_to_pdf, ConvertOptions, ProgressListener};

struct Print;
impl ProgressListener for Print {
    fn on_progress(&self, fraction: f32) -> bool {
        eprint!("\r{:5.1}%", fraction * 100.0);
        true
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    eprintln!("{}", convert_core::core_version());
    let result = if args.first().map(String::as_str) == Some("--pdf") {
        images_to_pdf(args[2..].to_vec(), args[1].clone(), ConvertOptions::default(), Some(Box::new(Print)))
    } else {
        let mut o = ConvertOptions::default();
        if let Some(q) = args.get(2) { o.quality = q.parse().expect("quality"); }
        if let Some(m) = args.get(3) { o.max_dimension = m.parse().expect("max_dimension"); }
        convert(args[0].clone(), args[1].clone(), o, Some(Box::new(Print)))
    };
    eprintln!();
    if let Err(e) = result {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}
