//! Owned copies of what the sound server sends.
//!
//! The data given to an introspection callback is only valid during the
//! callback, so we copy the fields we need into these plain types. They are
//! also easy to build in tests, without a sound server.

/// Everything read from the sound server in one pass.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// Name of the default sink (the default output).
    pub default_sink: Option<String>,
    /// Name of the default source (the default input).
    pub default_source: Option<String>,
    pub sinks: Vec<Sink>,
    pub sources: Vec<Source>,
    pub sink_inputs: Vec<SinkInput>,
    pub source_outputs: Vec<SourceOutput>,
}

/// Clues about the hardware of a device.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct DeviceProps {
    /// `device.form_factor`
    pub form_factor: Option<String>,
    /// `device.bus`
    pub bus: Option<String>,
    /// Name of the active port.
    pub port: Option<String>,
}

/// A sink: a device that plays sound.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Sink {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub props: DeviceProps,
    /// Average volume over the channels, in sound server units.
    pub volume: u32,
    pub mute: bool,
}

/// A source: a device that gives sound, or the monitor of a sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Source {
    pub index: u32,
    pub name: String,
    pub description: String,
    pub props: DeviceProps,
    /// Index of the sink this source copies, when it is a monitor.
    pub monitor_of_sink: Option<u32>,
    /// Name of the sink this source copies, when the server gives it.
    pub monitor_of_sink_name: Option<String>,
    pub volume: u32,
    pub mute: bool,
}

/// Properties of a stream that tell which application owns it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(crate) struct StreamProps {
    /// `application.name`
    pub app_name: Option<String>,
    /// `application.process.binary`
    pub binary: Option<String>,
    /// `application.icon_name`
    pub icon_name: Option<String>,
    /// `application.id`, for example `org.gnome.VolumeControl`.
    pub app_id: Option<String>,
    /// `media.name`
    pub media_name: Option<String>,
    /// Name of the stream itself.
    pub stream_name: Option<String>,
    /// The client that owns the stream. `None` for a stream that a module
    /// of the server made itself (loopback, remap, echo cancel).
    pub client: Option<u32>,
}

/// A sink input: an application stream that plays on a sink.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SinkInput {
    pub index: u32,
    /// Index of the sink it plays on.
    pub sink: u32,
    pub props: StreamProps,
    pub volume: u32,
    pub mute: bool,
}

/// A source output: an application stream that records from a source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SourceOutput {
    pub index: u32,
    /// Index of the source it records from.
    pub source: u32,
    pub props: StreamProps,
    pub volume: u32,
    pub mute: bool,
    /// Resample method: `peaks` for a level meter on PulseAudio.
    pub resample_method: Option<String>,
}
