use super::{AudioDefaults, DefaultAudioDevice, DefaultAudioError};
use ::windows::{
    Win32::{
        Media::Audio::{
            EDataFlow, IMMDeviceEnumerator, MMDeviceEnumerator, eCapture, eCommunications,
            eConsole, eRender,
        },
        System::Com::{
            CLSCTX_ALL, COINIT_MULTITHREADED, CoCreateInstance, CoInitializeEx, CoTaskMemFree,
            CoUninitialize,
        },
    },
    core::PWSTR,
};

struct Com(std::marker::PhantomData<std::rc::Rc<()>>);
impl Drop for Com {
    fn drop(&mut self) {
        unsafe { CoUninitialize() };
    }
}
struct EndpointId(PWSTR);
impl Drop for EndpointId {
    fn drop(&mut self) {
        unsafe { CoTaskMemFree(Some(self.0.0.cast())) };
    }
}

fn endpoint(
    enumerator: &IMMDeviceEnumerator,
    flow: EDataFlow,
) -> Result<Option<DefaultAudioDevice>, DefaultAudioError> {
    // Calls use the communications default; if it is absent, use the OS console
    // default. Never substitute an arbitrary enumerated device.
    let device = unsafe {
        enumerator
            .GetDefaultAudioEndpoint(flow, eCommunications)
            .or_else(|_| enumerator.GetDefaultAudioEndpoint(flow, eConsole))
    };
    let Ok(device) = device else {
        return Ok(None);
    };
    let id = EndpointId(unsafe { device.GetId() }.map_err(|_| DefaultAudioError::Unavailable)?);
    if id.0.is_null() {
        return Err(DefaultAudioError::InvalidIdentifier);
    }
    let id = unsafe { id.0.to_string() }.map_err(|_| DefaultAudioError::InvalidIdentifier)?;
    if id.is_empty() || id.len() > 4096 {
        return Err(DefaultAudioError::InvalidIdentifier);
    }
    Ok(Some(DefaultAudioDevice { id, name: None }))
}
pub fn audio_defaults() -> Result<AudioDefaults, DefaultAudioError> {
    // Core invokes this on a blocking worker. Keep every COM object and the
    // matching uninitialize on this same thread, including error paths.
    unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) }
        .ok()
        .map_err(|_| DefaultAudioError::Unavailable)?;
    let _com = Com(Default::default());
    let enumerator: IMMDeviceEnumerator =
        unsafe { CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL) }
            .map_err(|_| DefaultAudioError::Unavailable)?;
    Ok(AudioDefaults {
        input: endpoint(&enumerator, eCapture)?,
        output: endpoint(&enumerator, eRender)?,
    })
}
