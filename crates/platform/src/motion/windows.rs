use tokio::sync::watch;
pub(super) fn read() -> bool {
    use ::windows::Win32::UI::WindowsAndMessaging::{
        SPI_GETCLIENTAREAANIMATION, SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS, SystemParametersInfoW,
    };
    use ::windows::core::BOOL;
    let mut enabled = BOOL(1);
    // SAFETY: the SPI action writes one BOOL to this live, aligned output value.
    unsafe {
        SystemParametersInfoW(
            SPI_GETCLIENTAREAANIMATION,
            0,
            Some((&mut enabled as *mut BOOL).cast()),
            SYSTEM_PARAMETERS_INFO_UPDATE_FLAGS(0),
        )
    }
    .is_ok()
        && !enabled.as_bool()
}
pub(super) fn observe(sender: watch::Sender<bool>) {
    loop {
        sender.send_if_modified(|current| {
            let next = read();
            let changed = *current != next;
            *current = next;
            changed
        });
        std::thread::sleep(std::time::Duration::from_secs(2));
    }
}
