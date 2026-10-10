fn main() {
    println!("cargo:rerun-if-changed=assets/windows/app-icon.rc");
    println!("cargo:rerun-if-changed=assets/icons/io.github.wsyxbcl.waxbill.ico");

    // The resource compiler needs a Windows host (as in CI); a cross-check from elsewhere builds without the icon.
    let windows_host = std::env::var("HOST").is_ok_and(|host| host.contains("windows"));
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") && windows_host {
        embed_resource::compile("assets/windows/app-icon.rc", embed_resource::NONE)
            .manifest_optional()
            .expect("failed to compile Windows icon resource");
    }
}
