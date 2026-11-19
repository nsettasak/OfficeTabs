use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=assets/app.rc");
    println!("cargo:rerun-if-changed=assets/app.manifest");
    println!("cargo:rerun-if-changed=assets/OfficeTabs.ico");
    println!("cargo:rerun-if-env-changed=OFFICETABS_MSI");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    let payload = env::var_os("OFFICETABS_MSI").map(PathBuf::from);
    if let Some(ref p) = payload {
        println!("cargo:rerun-if-changed={}", p.display());
    }
    std::fs::write(
        output.join("payload.rs"),
        match payload {
            Some(p) => format!("pub static MSI: &[u8] = include_bytes!({:?});", p),
            None => "pub static MSI: &[u8] = &[];".into(),
        },
    )
    .unwrap();
    if env::var("CARGO_CFG_TARGET_OS").unwrap() == "windows" {
        println!("cargo:rerun-if-env-changed=RC_EXE");
        let rc = env::var_os("RC_EXE").map(PathBuf::from).unwrap_or_else(|| {
            let kits = PathBuf::from(
                env::var_os("ProgramFiles(x86)")
                    .unwrap_or_else(|| r"C:\Program Files (x86)".into()),
            )
            .join(r"Windows Kits\10\bin");
            let mut candidates: Vec<_> = std::fs::read_dir(kits)
                .into_iter()
                .flatten()
                .filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.join("x64/rc.exe").is_file())
                .collect();
            candidates.sort_by_key(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .split('.')
                    .map(|s| s.parse::<u32>().unwrap_or(0))
                    .collect::<Vec<_>>()
            });
            candidates
                .pop()
                .expect("Install a Windows SDK or set RC_EXE")
                .join("x64/rc.exe")
        });
        let res = output.join("app.res");
        let status = Command::new(rc)
            .arg("/nologo")
            .arg(format!("/fo{}", res.display()))
            .arg("assets/app.rc")
            .status()
            .expect("Windows SDK resource compiler missing; set RC_EXE");
        assert!(status.success(), "resource compile failed");
        println!("cargo:rustc-link-arg={}", res.display());
        // COM requires DllGetClassObject/DllCanUnloadNow to stay public, so they cannot be
        // marked PRIVATE in the generated .def. Silence link.exe's complaint about exactly
        // that (LNK4104) rather than hiding the exports.
        if env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
            println!("cargo:rustc-link-arg=/ignore:4104");
        }
    }
}
