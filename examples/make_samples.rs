use std::{fs, io, path::Path};

use engine_pdf::samples::Sample;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    if !cfg!(target_os = "macos") {
        return Err(engine_pdf::Error::Unsupported.into());
    }
    let out = Path::new(env!("CARGO_MANIFEST_DIR")).join("samples/out");
    fs::create_dir_all(&out)?;
    for sample in Sample::ALL {
        let path = out.join(sample.filename());
        let bytes = sample.generate().map_err(|error| {
            io::Error::other(format!("生成 {} 失败。{error}", sample.filename()))
        })?;
        fs::write(&path, bytes)?;
        println!("{}", path.display());
    }
    Ok(())
}
