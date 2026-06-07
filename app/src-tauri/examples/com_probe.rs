#[path = "../src/photoshop.rs"]
mod photoshop;

use photoshop::PhotoshopClient;

fn main() {
    let mut args = std::env::args().skip(1);
    let expected_path = args.next();
    let font = args.next();

    let ps = PhotoshopClient::active(expected_path.as_deref()).unwrap_or_else(|err| {
        eprintln!("status=ERR message={err}");
        std::process::exit(1);
    });

    println!(
        "status=OK path={} version={}",
        ps.path().unwrap_or_default(),
        ps.version().unwrap_or_default()
    );

    let fonts = ps.list_fonts().unwrap_or_else(|err| {
        eprintln!("list=ERR message={err}");
        std::process::exit(2);
    });
    println!("list=OK count={}", fonts.len());
    if let Some(first) = fonts.first() {
        println!(
            "first name={} family={} style={} postScriptName={}",
            first.name, first.family, first.style, first.post_script_name
        );
    }

    if let Some(font) = font {
        let result = ps.apply_font(&font).unwrap_or_else(|err| {
            eprintln!("apply=ERR font={font} message={err}");
            std::process::exit(3);
        });
        println!("apply={} font={}", result, font);
        if result != "0" {
            std::process::exit(4);
        }
    }
}
