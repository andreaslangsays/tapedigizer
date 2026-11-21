#[allow(dead_code)]
pub mod i18n;
#[allow(dead_code)]
pub mod project;
#[allow(dead_code)]
pub mod splitter;
#[allow(dead_code)]
pub mod audio {
    pub mod denoiser;
    pub mod meter;
    pub mod processing;
}

#[cfg(target_os = "macos")]
pub mod au_plugin;
