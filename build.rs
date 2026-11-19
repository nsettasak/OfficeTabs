use std::{env, path::PathBuf, process::Command};
fn main() {
    println!("cargo:rerun-if-changed=app.rc");
    println!("cargo:rerun-if-changed=app.manifest");
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
        // Single source of truth for the version: Cargo.toml. The generated
        // include keeps the Win32 resource, the MSI and the release name in step.
        let version = env::var("CARGO_PKG_VERSION").unwrap();
        let numbers: Vec<u32> = version
            .split('.')
            .map(|part| {
                part.chars()
                    .take_while(char::is_ascii_digit)
                    .collect::<String>()
                    .parse()
                    .unwrap_or(0)
            })
            .take(3)
            .collect();
        let quad = format!(
            "{},{},{},0",
            numbers.first().copied().unwrap_or(0),
            numbers.get(1).copied().unwrap_or(0),
            numbers.get(2).copied().unwrap_or(0)
        );
        let dotted = quad.replace(',', ".");
        std::fs::write(
            output.join("version.rc"),
            format!(
                "1 VERSIONINFO\n\
                 FILEVERSION {quad}\n\
                 PRODUCTVERSION {quad}\n\
                 FILEOS 0x40004\n\
                 FILETYPE 0x1\n\
                 BEGIN\n\
                  BLOCK \"StringFileInfo\"\n\
                  BEGIN\n\
                   BLOCK \"040904b0\"\n\
                   BEGIN\n\
                    VALUE \"CompanyName\", \"OfficeTabs\\0\"\n\
                    VALUE \"FileDescription\", \"OfficeTabs Native Preview\\0\"\n\
                    VALUE \"FileVersion\", \"{dotted}\\0\"\n\
                    VALUE \"ProductName\", \"OfficeTabs\\0\"\n\
                    VALUE \"ProductVersion\", \"{dotted}\\0\"\n\
                   END\n\
                  END\n\
                  BLOCK \"VarFileInfo\"\n\
                  BEGIN\n\
                   VALUE \"Translation\", 0x409, 1200\n\
                  END\n\
                 END\n"
            ),
        )
        .unwrap();
        println!("cargo:rerun-if-changed=build.rs");
        let res = output.join("app.res");
        let status = Command::new(rc)
            .arg("/nologo")
            .arg(format!("/fo{}", res.display()))
            .arg(format!("/i{}", output.display()))
            // Include the assets directory so app.rc can name the icon without a path.
            .arg(format!("/i{}", "assets"))
            .arg("app.rc")
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
