//! PulseAudio protocol also covers PipeWire's PulseAudio compatibility service.
//! Read only the two default names/descriptions; do not copy server user/host or
//! cookie fields, create a stream, change routing, or autospawn a daemon.
use super::{AudioDefaults, DefaultAudioDevice, DefaultAudioError};
use libpulse_binding::{
    callbacks::ListResult,
    context::{Context, FlagSet, State},
    mainloop::standard::{IterateResult, Mainloop},
};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
    time::{Duration, Instant},
};

fn pump(mainloop: &mut Mainloop, deadline: Instant) -> Result<(), DefaultAudioError> {
    if Instant::now() >= deadline {
        return Err(DefaultAudioError::Timeout);
    }
    match mainloop.iterate(false) {
        IterateResult::Success(_) => {
            std::thread::sleep(Duration::from_millis(1));
            Ok(())
        }
        _ => Err(DefaultAudioError::Unavailable),
    }
}
fn description(
    context: &Context,
    mainloop: &mut Mainloop,
    name: &str,
    input: bool,
    deadline: Instant,
) -> Option<String> {
    let value = Rc::new(RefCell::new(None));
    let done = Rc::new(Cell::new(false));
    if input {
        let (output, finished) = (value.clone(), done.clone());
        let mut operation = context
            .introspect()
            .get_source_info_by_name(name, move |result| match result {
                ListResult::Item(info) => {
                    *output.borrow_mut() = info.description.as_ref().map(|s| s.to_string())
                }
                ListResult::End | ListResult::Error => finished.set(true),
            });
        while !done.get() {
            if pump(mainloop, deadline).is_err() {
                operation.cancel();
                break;
            }
        }
    } else {
        let (output, finished) = (value.clone(), done.clone());
        let mut operation = context
            .introspect()
            .get_sink_info_by_name(name, move |result| match result {
                ListResult::Item(info) => {
                    *output.borrow_mut() = info.description.as_ref().map(|s| s.to_string())
                }
                ListResult::End | ListResult::Error => finished.set(true),
            });
        while !done.get() {
            if pump(mainloop, deadline).is_err() {
                operation.cancel();
                break;
            }
        }
    }
    value.take().filter(|s| !s.is_empty() && s.len() <= 4096)
}
pub fn audio_defaults() -> Result<AudioDefaults, DefaultAudioError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    let mut mainloop = Mainloop::new().ok_or(DefaultAudioError::Unavailable)?;
    let mut context = Context::new(&mainloop, "Twayblade default device query")
        .ok_or(DefaultAudioError::Unavailable)?;
    context
        .connect(None, FlagSet::NOAUTOSPAWN, None)
        .map_err(|_| DefaultAudioError::Unavailable)?;
    while context.get_state() != State::Ready {
        if matches!(context.get_state(), State::Failed | State::Terminated) {
            return Err(DefaultAudioError::Unavailable);
        }
        pump(&mut mainloop, deadline)?;
    }
    let names = Rc::new(RefCell::new(None));
    let output = names.clone();
    let mut operation = context.introspect().get_server_info(move |info| {
        *output.borrow_mut() = Some((
            info.default_source_name.as_ref().map(|s| s.to_string()),
            info.default_sink_name.as_ref().map(|s| s.to_string()),
        ));
    });
    loop {
        let waiting = names.borrow().is_none();
        if !waiting {
            break;
        }
        if let Err(error) = pump(&mut mainloop, deadline) {
            operation.cancel();
            return Err(error);
        }
    }
    let (input, output) = names.take().ok_or(DefaultAudioError::Unavailable)?;
    let mut device =
        |id: Option<String>, input| -> Result<Option<DefaultAudioDevice>, DefaultAudioError> {
            let Some(id) = id else {
                return Ok(None);
            };
            if id.is_empty() || id.len() > 4096 || id.contains('\0') {
                return Err(DefaultAudioError::InvalidIdentifier);
            }
            let name = description(&context, &mut mainloop, &id, input, deadline);
            Ok(Some(DefaultAudioDevice { id, name }))
        };
    let defaults = AudioDefaults {
        input: device(input, true)?,
        output: device(output, false)?,
    };
    context.disconnect();
    Ok(defaults)
}
