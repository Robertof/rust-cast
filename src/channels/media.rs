use std::{
    borrow::Cow,
    io::{Read, Write},
    str::FromStr,
    string::ToString, ops::Range,
};

use crate::{
    cast::proxies,
    errors::Error,
    message_manager::{CastMessage, CastMessagePayload, MessageManager},
    Lrc,
};

const CHANNEL_NAMESPACE: &str = "urn:x-cast:com.google.cast.media";

const MESSAGE_TYPE_GET_STATUS: &str = "GET_STATUS";
const MESSAGE_TYPE_LOAD: &str = "LOAD";
const MESSAGE_TYPE_PLAY: &str = "PLAY";
const MESSAGE_TYPE_PAUSE: &str = "PAUSE";
const MESSAGE_TYPE_STOP: &str = "STOP";
const MESSAGE_TYPE_SEEK: &str = "SEEK";
const MESSAGE_TYPE_EDIT_TRACKS_INFO: &str = "EDIT_TRACKS_INFO";
const MESSAGE_TYPE_MEDIA_STATUS: &str = "MEDIA_STATUS";
const MESSAGE_TYPE_LOAD_CANCELLED: &str = "LOAD_CANCELLED";
const MESSAGE_TYPE_LOAD_FAILED: &str = "LOAD_FAILED";
const MESSAGE_TYPE_INVALID_PLAYER_STATE: &str = "INVALID_PLAYER_STATE";
const MESSAGE_TYPE_INVALID_REQUEST: &str = "INVALID_REQUEST";
const MESSAGE_TYPE_ERROR: &str = "ERROR";

/// Describes the way cast device should stream content.
#[derive(Copy, Clone, Debug)]
pub enum StreamType {
    /// This variant allows cast device to automatically choose whatever way it's most comfortable
    /// with.
    None,
    /// Cast device should buffer some portion of the content and only then start streaming.
    Buffered,
    /// Cast device should display content as soon as it gets any portion of it.
    Live,
}

impl FromStr for StreamType {
    type Err = Error;

    fn from_str(s: &str) -> Result<StreamType, Error> {
        match s {
            "BUFFERED" | "buffered" => Ok(StreamType::Buffered),
            "LIVE" | "live" => Ok(StreamType::Live),
            _ => Ok(StreamType::None),
        }
    }
}

impl ToString for StreamType {
    fn to_string(&self) -> String {
        let stream_type = match *self {
            StreamType::None => "NONE",
            StreamType::Buffered => "BUFFERED",
            StreamType::Live => "LIVE",
        };

        stream_type.to_string()
    }
}

pub type CustomData = serde_json::Value;

/// Options for loading media, see `MediaChannel::load_with_options`.
#[derive(Clone, Debug, Default)]
pub struct LoadOptions {
    /// Position (in seconds) to start playback from. If not set, the receiver picks one, usually
    /// the beginning for on demand content and the live edge for live content.
    pub current_time: Option<f32>,
    /// Custom data for the receiver application, attached to the media.
    pub custom_data: Option<CustomData>,
}

/// Generic, movie, TV show, music track, or photo metadata.
#[derive(Clone, Debug)]
pub enum Metadata {
    Generic(GenericMediaMetadata),
    Movie(MovieMediaMetadata),
    TvShow(TvShowMediaMetadata),
    MusicTrack(MusicTrackMediaMetadata),
    Photo(PhotoMediaMetadata),
}

/// Generic media metadata.
///
/// See also the [`GenericMediaMetadata` Cast reference](https://developers.google.com/cast/docs/reference/messages#GenericMediaMetadata).
#[derive(Clone, Debug)]
pub struct GenericMediaMetadata {
    /// Descriptive title of the content.
    pub title: Option<String>,
    /// Descriptive subtitle of the content.
    pub subtitle: Option<String>,
    /// Zero or more URLs to an image associated with the content.
    pub images: Vec<Image>,
    /// Date and time the content was released, formatted as ISO 8601.
    pub release_date: Option<String>,
}

/// Movie media metadata.
///
/// See also the [`MovieMediaMetadata` Cast reference](https://developers.google.com/cast/docs/reference/messages#MovieMediaMetadata).
#[derive(Clone, Debug)]
pub struct MovieMediaMetadata {
    /// Title of the movie.
    pub title: Option<String>,
    /// Subtitle of the movie.
    pub subtitle: Option<String>,
    /// Studio which released the movie.
    pub studio: Option<String>,
    /// Zero or more URLs to an image associated with the content.
    pub images: Vec<Image>,
    /// Date and time the movie was released, formatted as ISO 8601.
    pub release_date: Option<String>,
}

/// TV show media metadata.
///
/// See also the [`TvShowMediaMetadata` Cast reference](https://developers.google.com/cast/docs/reference/messages#TvShowMediaMetadata).
#[derive(Clone, Debug)]
pub struct TvShowMediaMetadata {
    /// Title of the TV series.
    pub series_title: Option<String>,
    /// Title of the episode.
    pub episode_title: Option<String>,
    /// Season number of the TV show.
    pub season: Option<u32>,
    /// Episode number (in the season) of the episode.
    pub episode: Option<u32>,
    /// Zero or more URLs to an image associated with the content.
    pub images: Vec<Image>,
    /// Date and time this episode was released, formatted as ISO 8601.
    pub original_air_date: Option<String>,
}

/// Music track media metadata.
///
/// See also the [`MusicTrackMediaMetadata` Cast reference](https://developers.google.com/cast/docs/reference/messages#MusicTrackMediaMetadata).
#[derive(Clone, Debug)]
pub struct MusicTrackMediaMetadata {
    /// Album or collection from which the track is taken.
    pub album_name: Option<String>,
    /// Name of the track (for example, song title).
    pub title: Option<String>,
    /// Name of the artist associated with the album featuring this track.
    pub album_artist: Option<String>,
    /// Name of the artist associated with the track.
    pub artist: Option<String>,
    /// Name of the composer associated with the track.
    pub composer: Option<String>,
    /// Number of the track on the album.
    pub track_number: Option<u32>,
    /// Number of the volume (for example, a disc) of the album.
    pub disc_number: Option<u32>,
    /// Zero or more URLs to an image associated with the content.
    pub images: Vec<Image>,
    /// Date and time the content was released, formatted as ISO 8601.
    pub release_date: Option<String>,
}

/// Photo media metadata.
///
/// See also the [`PhotoMediaMetadata` Cast reference](https://developers.google.com/cast/docs/reference/messages#PhotoMediaMetadata).
#[derive(Clone, Debug)]
pub struct PhotoMediaMetadata {
    /// Title of the photograph.
    pub title: Option<String>,
    /// Name of the photographer.
    pub artist: Option<String>,
    /// Verbal location where the photograph was taken, for example “Madrid, Spain”.
    pub location: Option<String>,
    /// Latitude and longitude of the location where the photograph was taken.
    pub latitude_longitude: Option<(f64, f64)>,
    /// Width and height of the photograph in pixels.
    pub dimensions: Option<(u32, u32)>,
    /// Date and time the photograph was taken, formatted as ISO 8601.
    pub creation_date_time: Option<String>,
}

/// Image URL and optionally size metadata.
///
/// This is the description of an image, including a small amount of metadata to
/// allow the sender application a choice of images, depending on how it will
/// render them. The height and width are optional on only one item in an array
/// of images.
///
/// See also the [`Image` Cast reference](https://developers.google.com/cast/docs/reference/messages#Image).
#[derive(Clone, Debug)]
pub struct Image {
    /// URL of the image.
    pub url: String,
    /// Width and height of the image.
    pub dimensions: Option<(u32, u32)>,
}

impl Image {
    pub fn new(url: String) -> Image {
        Image {
            url,
            dimensions: None,
        }
    }

    fn encode(&self) -> proxies::media::Image {
        proxies::media::Image {
            url: self.url.clone(),
            width: self.dimensions.map(|d| d.0),
            height: self.dimensions.map(|d| d.1),
        }
    }
}

/// Describes possible player states.
#[derive(Copy, Clone, Debug)]
pub enum PlayerState {
    /// Player has not been loaded yet.
    Idle,
    /// Player is actively playing content.
    Playing,
    /// Player is in PLAY mode but not actively playing content (currentTime is not changing).
    Buffering,
    /// Player is paused.
    Paused,
}

impl FromStr for PlayerState {
    type Err = Error;

    fn from_str(s: &str) -> Result<PlayerState, Error> {
        match s {
            "IDLE" => Ok(PlayerState::Idle),
            "PLAYING" => Ok(PlayerState::Playing),
            "BUFFERING" => Ok(PlayerState::Buffering),
            "PAUSED" => Ok(PlayerState::Paused),
            _ => Err(Error::Internal(format!("Unknown player state {}", s))),
        }
    }
}

impl ToString for PlayerState {
    fn to_string(&self) -> String {
        let player_state = match *self {
            PlayerState::Idle => "IDLE",
            PlayerState::Playing => "PLAYING",
            PlayerState::Buffering => "BUFFERING",
            PlayerState::Paused => "PAUSED",
        };

        player_state.to_string()
    }
}

/// Describes possible player idle reasons.
#[derive(Copy, Clone, Debug)]
pub enum IdleReason {
    /// A sender requested to stop playback using the STOP command.
    Cancelled,
    /// A sender requested playing a different media using the LOAD command.
    Interrupted,
    /// The media playback completed.
    Finished,
    /// The media was interrupted due to an error; For example, if the player could not download the
    /// media due to network issues.
    Error,
}

impl FromStr for IdleReason {
    type Err = Error;

    fn from_str(s: &str) -> Result<IdleReason, Error> {
        match s {
            "CANCELLED" => Ok(IdleReason::Cancelled),
            "INTERRUPTED" => Ok(IdleReason::Interrupted),
            "FINISHED" => Ok(IdleReason::Finished),
            "ERROR" => Ok(IdleReason::Error),
            _ => Err(Error::Internal(format!("Unknown idle reason {}", s))),
        }
    }
}

/// Describes the operation to perform with playback while seeking.
#[derive(Copy, Clone, Debug)]
pub enum ResumeState {
    /// Forces media to start.
    PlaybackStart,
    /// Forces media to pause.
    PlaybackPause,
}

impl FromStr for ResumeState {
    type Err = Error;

    fn from_str(s: &str) -> Result<ResumeState, Error> {
        match s {
            "PLAYBACK_START" | "start" => Ok(ResumeState::PlaybackStart),
            "PLAYBACK_PAUSE" | "pause" => Ok(ResumeState::PlaybackPause),
            _ => Err(Error::Internal(format!("Unknown resume state {}", s))),
        }
    }
}

impl ToString for ResumeState {
    fn to_string(&self) -> String {
        let resume_state = match *self {
            ResumeState::PlaybackStart => "PLAYBACK_START",
            ResumeState::PlaybackPause => "PLAYBACK_PAUSE",
        };

        resume_state.to_string()
    }
}

/// Media track type.
#[derive(Copy, Clone, Debug)]
pub enum TrackType {
    /// Text track.
    Text,
    /// Audio track.
    Audio,
    /// Video track.
    Video,
}

impl FromStr for TrackType {
    type Err = Error;

    fn from_str(s: &str) -> Result<TrackType, Error> {
        match s {
            "TEXT" => Ok(TrackType::Text),
            "AUDIO" => Ok(TrackType::Audio),
            "VIDEO" => Ok(TrackType::Video),
            _ => Err(Error::Internal(format!("Unknown track type {}", s))),
        }
    }
}

#[derive(Clone, Debug, Default)]
pub struct TrackSelection {
    /// Array of the Track trackIds that should be active.
    ///
    /// If it is not provided, the active tracks will not change.
    /// If the array is empty, no track will be active.
    pub active_track_ids: Option<Vec<i32>>,

    /// Flag to enable or disable text tracks.
    ///
    /// If false it will disable all text tracks, if true it will enable the first text track,
    /// or the previous active text tracks.
    /// This flag is ignored if activeTrackIds or language is provided.
    pub enable_text_tracks: Option<bool>,

    /// Language for the tracks that should be active.
    ///
    /// The language field will take precedence over activeTrackIds if both are specified.
    pub language: Option<String>,
}

/// This data structure describes track metadata information.
#[derive(Clone, Debug)]
pub struct Track {
    /// The unique identifier of the track within the context of a MediaInformation object.
    pub id: i32,
    /// Describes the the type of track.
    pub track_type: TrackType,
    /// A descriptive, human-readable name for the track, for example, Spanish.
    pub name: Option<String>,
    /// An RFC 5646 language tag. If the track subtype is Subtitles, this field is mandatory.
    pub language: Option<String>,
}

/// This data structure describes a media stream.
#[derive(Clone, Debug)]
pub struct Media {
    /// Service-specific identifier of the content currently loaded by the media player. This is a
    /// free form string and is specific to the application. In most cases, this will be the URL to
    /// the media, but the sender can choose to pass a string that the receiver can interpret
    /// properly. Max length: 1k.
    pub content_id: String,
    /// Describes the type of media artifact.
    pub stream_type: StreamType,
    /// MIME content type of the media being played.
    pub content_type: String,
    /// Generic, movie, TV show, music track, or photo metadata.
    pub metadata: Option<Metadata>,
    /// The media tracks.
    pub tracks: Vec<Track>,
    /// Duration of the currently playing stream in seconds.
    pub duration: Option<f32>,
}

/// Describes the current status of the media artifact with respect to the session.
#[derive(Clone, Debug)]
pub struct Status {
    /// Unique id of the request that requested the status.
    pub request_id: u32,
    /// Detailed status of every media status entry.
    pub entries: Vec<StatusEntry>,
}

/// Detailed status of the media artifact with respect to the session.
#[derive(Clone, Debug)]
pub struct StatusEntry {
    /// List of IDs corresponding to the active Tracks.
    pub active_track_ids: Vec<i32>,
    /// Unique ID for the playback of this specific session. This ID is set by the receiver at LOAD
    /// and can be used to identify a specific instance of a playback. For example, two playbacks of
    /// "Wish you were here" within the same session would each have a unique mediaSessionId.
    pub media_session_id: i32,
    /// Full description of the content that is being played back. Only be returned in a status
    /// messages if the Media has changed.
    pub media: Option<Media>,
    /// Seekable range of a live or event stream. It uses relative media time in seconds.
    /// It will be undefined for VOD streams.
    pub live_seekable_range: Option<Range<f32>>,
    /// Indicates whether the media time is progressing, and at what rate. This is independent of
    /// the player state since the media time can stop in any state. 1.0 is regular time, 0.5 is
    /// slow motion.
    pub playback_rate: f32,
    /// Describes the state of the player.
    pub player_state: PlayerState,
    /// If the player_state is IDLE and the reason it became IDLE is known, this property is
    /// provided. If the player is IDLE because it just started, this property will not be provided.
    /// If the player is in any other state this property should not be provided.
    pub idle_reason: Option<IdleReason>,
    /// The current position of the media player since the beginning of the content, in seconds.
    /// If this a live stream content, then this field represents the time in seconds from the
    /// beginning of the event that should be known to the player.
    pub current_time: Option<f32>,
    /// Flags describing which media commands the media player supports:
    /// * `1` `Pause`;
    /// * `2` `Seek`;
    /// * `4` `Stream volume`;
    /// * `8` `Stream mute`;
    /// * `16` `Skip forward`;
    /// * `32` `Skip backward`;
    /// * `1 << 12` `Unknown`;
    /// * `1 << 13` `Unknown`;
    /// * `1 << 18` `Unknown`.
    /// Combinations are described as summations; for example, Pause+Seek+StreamVolume+Mute == 15.
    pub supported_media_commands: u32,
}

/// Describes the load cancelled error.
#[derive(Copy, Clone, Debug)]
pub struct LoadCancelled {
    /// Unique id of the request that caused this error.
    pub request_id: u32,
}

/// Describes the load failed error.
#[derive(Copy, Clone, Debug)]
pub struct LoadFailed {
    /// Unique id of the request that caused this error.
    pub request_id: u32,
}

/// Describes the invalid player state error.
#[derive(Copy, Clone, Debug)]
pub struct InvalidPlayerState {
    /// Unique id of the request that caused this error.
    pub request_id: u32,
}

/// Describes the invalid request error.
#[derive(Clone, Debug)]
pub struct InvalidRequest {
    /// Unique id of the invalid request.
    pub request_id: u32,
    /// Description of the invalid request reason if available.
    pub reason: Option<String>,
}

/// The media error encountered during media operations.
#[derive(Clone, Debug, PartialEq)]
pub struct MediaError {
    /// The detailed error code associated with the media error.
    pub detailed_error_code: MediaDetailedErrorCode,
    /// The type of the error message.
    pub message_type: String,
}

/// The detailed media error code.
/// https://developers.google.com/android/reference/com/google/android/gms/cast/MediaError.DetailedErrorCode#constants
#[derive(Clone, Debug, PartialEq)]
pub enum MediaDetailedErrorCode {
    /// An error occurs outside of the framework (e.g., if an event handler throws an error).
    App = 900,
    /// Break clip load interceptor fails.
    BreakClipLoadingError = 901,
    /// Break seek interceptor fails.
    BreakSeekInterceptorError = 902,
    /// A DASH manifest contains invalid segment info.
    DashInvalidSegmentInfo = 423,
    /// A DASH manifest is missing a MimeType.
    DashManifestNoMimeType = 422,
    /// A DASH manifest is missing periods.
    DashManifestNoPeriods = 421,
    /// An unknown error occurs while parsing a DASH manifest.
    DashManifestUnknown = 420,
    /// An unknown network error occurs while handling a DASH stream.
    DashNetwork = 321,
    /// A DASH stream is missing an init.
    DashNoInit = 322,
    /// Returned when an unknown error occurs.
    Generic = 999,
    /// An error occurs while parsing an HLS master manifest.
    HlsManifestMaster = 411,
    /// An error occurs while parsing an HLS playlist.
    HlsManifestPlaylist = 412,
    /// An HLS segment is invalid.
    HlsNetworkInvalidSegment = 315,
    /// A request for an HLS key fails before it is sent.
    HlsNetworkKeyLoad = 314,
    /// An HLS master playlist fails to download.
    HlsNetworkMasterPlaylist = 311,
    /// An HLS key fails to download.
    HlsNetworkNoKeyResponse = 313,
    /// An HLS playlist fails to download.
    HlsNetworkPlaylist = 312,
    /// An HLS segment fails to parse.
    HlsSegmentParsing = 316,
    /// When an image fails to load.
    ImageError = 903,
    /// A load command failed.
    LoadFailed = 905,
    /// A load was interrupted by an unload, or by another load.
    LoadInterrupted = 904,
    /// An unknown error occurs while parsing a manifest.
    ManifestUnknown = 400,
    /// There is a media keys failure due to a network issue.
    MediakeysNetwork = 201,
    /// There is an unknown error with media keys.
    MediakeysUnknown = 200,
    /// A MediaKeySession object cannot be created.
    MediakeysUnsupported = 202,
    /// Crypto failed.
    MediakeysWebcrypto = 203,
    /// The fetching process for the media resource was aborted by the user agent at the user's request.
    MediaAborted = 101,
    /// An error occurred while decoding the media resource, after the resource was established to be usable.
    MediaDecode = 102,
    /// An error message was sent to the sender.
    MediaErrorMessage = 906,
    /// A network error caused the user agent to stop fetching the media resource, after the resource was established to be usable.
    MediaNetwork = 103,
    /// The media resource indicated by the src attribute was not suitable.
    MediaSrcNotSupported = 104,
    /// The HTMLMediaElement throws an error, but CAF does not recognize the specific error.
    MediaUnknown = 100,
    /// There was an unknown network issue.
    NetworkUnknown = 300,
    /// A segment fails to download.
    SegmentNetwork = 301,
    /// An unknown segment error occurs.
    SegmentUnknown = 500,
    /// An error occurs while parsing a Smooth manifest.
    SmoothManifest = 431,
    /// An unknown network error occurs while handling a Smooth stream.
    SmoothNetwork = 331,
    /// A Smooth stream is missing media data.
    SmoothNoMediaData = 332,
    /// A source buffer cannot be added to the MediaSource.
    SourceBufferFailure = 110,
    /// An unknown error occurred with a text stream.
    TextUnknown = 600,
}

impl TryFrom<i32> for MediaDetailedErrorCode {
    type Error = Error;

    fn try_from(value: i32) -> Result<Self, Self::Error> {
        match value {
            900 => Ok(MediaDetailedErrorCode::App),
            901 => Ok(MediaDetailedErrorCode::BreakClipLoadingError),
            902 => Ok(MediaDetailedErrorCode::BreakSeekInterceptorError),
            423 => Ok(MediaDetailedErrorCode::DashInvalidSegmentInfo),
            422 => Ok(MediaDetailedErrorCode::DashManifestNoMimeType),
            421 => Ok(MediaDetailedErrorCode::DashManifestNoPeriods),
            420 => Ok(MediaDetailedErrorCode::DashManifestUnknown),
            321 => Ok(MediaDetailedErrorCode::DashNetwork),
            322 => Ok(MediaDetailedErrorCode::DashNoInit),
            999 => Ok(MediaDetailedErrorCode::Generic),
            411 => Ok(MediaDetailedErrorCode::HlsManifestMaster),
            412 => Ok(MediaDetailedErrorCode::HlsManifestPlaylist),
            315 => Ok(MediaDetailedErrorCode::HlsNetworkInvalidSegment),
            314 => Ok(MediaDetailedErrorCode::HlsNetworkKeyLoad),
            311 => Ok(MediaDetailedErrorCode::HlsNetworkMasterPlaylist),
            313 => Ok(MediaDetailedErrorCode::HlsNetworkNoKeyResponse),
            312 => Ok(MediaDetailedErrorCode::HlsNetworkPlaylist),
            316 => Ok(MediaDetailedErrorCode::HlsSegmentParsing),
            903 => Ok(MediaDetailedErrorCode::ImageError),
            905 => Ok(MediaDetailedErrorCode::LoadFailed),
            904 => Ok(MediaDetailedErrorCode::LoadInterrupted),
            400 => Ok(MediaDetailedErrorCode::ManifestUnknown),
            201 => Ok(MediaDetailedErrorCode::MediakeysNetwork),
            200 => Ok(MediaDetailedErrorCode::MediakeysUnknown),
            202 => Ok(MediaDetailedErrorCode::MediakeysUnsupported),
            203 => Ok(MediaDetailedErrorCode::MediakeysWebcrypto),
            101 => Ok(MediaDetailedErrorCode::MediaAborted),
            102 => Ok(MediaDetailedErrorCode::MediaDecode),
            906 => Ok(MediaDetailedErrorCode::MediaErrorMessage),
            103 => Ok(MediaDetailedErrorCode::MediaNetwork),
            104 => Ok(MediaDetailedErrorCode::MediaSrcNotSupported),
            100 => Ok(MediaDetailedErrorCode::MediaUnknown),
            300 => Ok(MediaDetailedErrorCode::NetworkUnknown),
            301 => Ok(MediaDetailedErrorCode::SegmentNetwork),
            500 => Ok(MediaDetailedErrorCode::SegmentUnknown),
            431 => Ok(MediaDetailedErrorCode::SmoothManifest),
            331 => Ok(MediaDetailedErrorCode::SmoothNetwork),
            332 => Ok(MediaDetailedErrorCode::SmoothNoMediaData),
            110 => Ok(MediaDetailedErrorCode::SourceBufferFailure),
            600 => Ok(MediaDetailedErrorCode::TextUnknown),
            _ => Err(Error::Parsing(format!(
                "media error code {} is not supported",
                value
            ))),
        }
    }
}

/// Represents all currently supported incoming messages that media channel can handle.
#[derive(Clone, Debug)]
pub enum MediaResponse {
    /// Statuses of the currently active media.
    Status(Status),
    /// Sent when the load request was cancelled (a second load request was received).
    LoadCancelled(LoadCancelled),
    /// Sent when the load request failed. The player state will be IDLE.
    LoadFailed(LoadFailed),
    /// Sent when the request by the sender can not be fulfilled because the player is not in a
    /// valid state. For example, if the application has not created a media element yet.
    InvalidPlayerState(InvalidPlayerState),
    /// Error indicating that request is not valid.
    InvalidRequest(InvalidRequest),
    /// The media error that occurred while executing a media operation on the media channel.
    Error(MediaError),
    /// Used every time when channel can't parse the message. Associated data contains `type` string
    /// field and raw JSON data returned from cast device.
    NotImplemented(String, serde_json::Value),
}

pub struct MediaChannel<'a, W>
where
    W: Read + Write,
{
    sender: Cow<'a, str>,
    message_manager: Lrc<MessageManager<W>>,
}

impl<'a, W> MediaChannel<'a, W>
where
    W: Read + Write,
{
    pub fn new<S>(sender: S, message_manager: Lrc<MessageManager<W>>) -> MediaChannel<'a, W>
    where
        S: Into<Cow<'a, str>>,
    {
        MediaChannel {
            sender: sender.into(),
            message_manager,
        }
    }

    /// Retrieves status of the cast device media session.
    ///
    /// # Arguments
    ///
    /// * `destination` - `protocol` identifier of specific app media session;
    /// * `media_session_id` - Media session ID of the media for which the media status should be
    /// returned. If none is provided, then the status for all media session IDs will be provided.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn get_status<S>(
        &self,
        destination: S,
        media_session_id: Option<i32>,
    ) -> Result<Status, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.send_get_status(destination, media_session_id)?;

        self.message_manager.receive_find_map(|message| {
            if !self.can_handle(message) {
                return Ok(None);
            }

            match self.parse(message)? {
                MediaResponse::Status(status) => {
                    if status.request_id == request_id {
                        return Ok(Some(status));
                    }
                }
                MediaResponse::InvalidRequest(error) => {
                    if error.request_id == request_id {
                        return Err(Error::Internal(format!(
                            "Invalid request ({}).",
                            error.reason.unwrap_or_else(|| "Unknown".to_string())
                        )));
                    }
                }
                _ => {}
            }

            Ok(None)
        })
    }

    /// Loads provided media to the application.
    ///
    /// # Arguments
    /// * `destination` - `protocol` of the application to load media with (e.g. `web-1`);
    /// * `session_id` - Current session identifier of the player application;
    /// * `media` - `Media` instance that describes the media we'd like to load.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn load<S>(&self, destination: S, session_id: S, media: &Media) -> Result<Status, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        self.load_with_custom_data(destination, session_id, media, None)
    }

    /// Loads provided media to the application with associated custom data.
    ///
    /// # Arguments
    /// * `destination` - `protocol` of the application to load media with (e.g. `web-1`);
    /// * `session_id` - Current session identifier of the player application;
    /// * `media` - `Media` instance that describes the media we'd like to load.
    /// * `custom_data` - serializable object with custom data.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn load_with_custom_data<S>(&self, destination: S, session_id: S, media: &Media, custom_data: Option<CustomData>) -> Result<Status, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        self.load_with_options(
            destination,
            session_id,
            media,
            LoadOptions {
                custom_data,
                ..Default::default()
            },
        )
    }

    /// Loads provided media to the application with the given options.
    ///
    /// # Arguments
    /// * `destination` - `protocol` of the application to load media with (e.g. `web-1`);
    /// * `session_id` - Current session identifier of the player application;
    /// * `media` - `Media` instance that describes the media we'd like to load;
    /// * `options` - `LoadOptions` such as the position to start playback from.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn load_with_options<S>(
        &self,
        destination: S,
        session_id: S,
        media: &Media,
        options: LoadOptions,
    ) -> Result<Status, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.message_manager.generate_request_id().get();

        let metadata = media.metadata.as_ref().map(|m| match *m {
            Metadata::Generic(ref x) => proxies::media::Metadata {
                title: x.title.clone(),
                subtitle: x.subtitle.clone(),
                images: x.images.iter().map(|i| i.encode()).collect(),
                release_date: x.release_date.clone(),
                ..proxies::media::Metadata::new(0)
            },
            Metadata::Movie(ref x) => proxies::media::Metadata {
                title: x.title.clone(),
                subtitle: x.subtitle.clone(),
                studio: x.studio.clone(),
                images: x.images.iter().map(|i| i.encode()).collect(),
                release_date: x.release_date.clone(),
                ..proxies::media::Metadata::new(1)
            },
            Metadata::TvShow(ref x) => proxies::media::Metadata {
                series_title: x.series_title.clone(),
                subtitle: x.episode_title.clone(),
                season: x.season,
                episode: x.episode,
                images: x.images.iter().map(|i| i.encode()).collect(),
                original_air_date: x.original_air_date.clone(),
                ..proxies::media::Metadata::new(2)
            },
            Metadata::MusicTrack(ref x) => proxies::media::Metadata {
                album_name: x.album_name.clone(),
                title: x.title.clone(),
                album_artist: x.album_artist.clone(),
                artist: x.artist.clone(),
                composer: x.composer.clone(),
                track_number: x.track_number,
                disc_number: x.disc_number,
                images: x.images.iter().map(|i| i.encode()).collect(),
                release_date: x.release_date.clone(),
                ..proxies::media::Metadata::new(3)
            },
            Metadata::Photo(ref x) => proxies::media::Metadata {
                title: x.title.clone(),
                artist: x.artist.clone(),
                location: x.location.clone(),
                latitude: x.latitude_longitude.map(|coord| coord.0),
                longitude: x.latitude_longitude.map(|coord| coord.1),
                width: x.dimensions.map(|dims| dims.0),
                height: x.dimensions.map(|dims| dims.1),
                creation_date_time: x.creation_date_time.clone(),
                ..proxies::media::Metadata::new(4)
            },
        });

        let payload = serde_json::to_string(&proxies::media::MediaRequest {
            request_id,
            session_id: session_id.into().to_string(),
            typ: MESSAGE_TYPE_LOAD.to_string(),

            media: proxies::media::Media {
                content_id: media.content_id.clone(),
                stream_type: media.stream_type.to_string(),
                content_type: media.content_type.clone(),
                metadata,
                duration: media.duration,
                tracks: vec![],
                custom_data: options.custom_data,
            },

            current_time: options.current_time.map(f64::from),
            autoplay: true,
            custom_data: proxies::media::CustomData::new(),
        })?;

        self.message_manager.send(CastMessage {
            namespace: CHANNEL_NAMESPACE.to_string(),
            source: self.sender.to_string(),
            destination: destination.into().to_string(),
            payload: CastMessagePayload::String(payload),
        })?;

        // Once media is loaded cast receiver device should emit status update event, or load failed
        // event if something went wrong.
        self.message_manager.receive_find_map(|message| {
            if !self.can_handle(message) {
                return Ok(None);
            }

            match self.parse(message)? {
                MediaResponse::Status(status) => {
                    if status.request_id == request_id {
                        return Ok(Some(status));
                    }

                    // [WORKAROUND] In some cases we don't receive response (e.g. from YouTube app),
                    // so let's just wait for the response with the media we're interested in and
                    // return it.
                    let has_media = {
                        status.entries.iter().any(|entry| {
                            if let Some(ref loaded_media) = entry.media {
                                return loaded_media.content_id == media.content_id;
                            }

                            false
                        })
                    };

                    if has_media {
                        return Ok(Some(status));
                    }
                }
                MediaResponse::LoadFailed(error) => {
                    if error.request_id == request_id {
                        return Err(Error::Internal("Failed to load media.".to_string()));
                    }
                }
                MediaResponse::LoadCancelled(error) => {
                    if error.request_id == request_id {
                        return Err(Error::Internal(
                            "Load cancelled by another request.".to_string(),
                        ));
                    }
                }
                MediaResponse::InvalidPlayerState(error) => {
                    if error.request_id == request_id {
                        return Err(Error::Internal(
                            "Load failed because of invalid player state.".to_string(),
                        ));
                    }
                }
                MediaResponse::InvalidRequest(error) => {
                    if error.request_id == request_id {
                        return Err(Error::Internal(format!(
                            "Load failed because of invalid media request (reason: {}).",
                            error.reason.unwrap_or_else(|| "UNKNOWN".to_string())
                        )));
                    }
                }
                _ => {}
            }

            Ok(None)
        })
    }

    /// Pauses playback of the current content. Triggers a STATUS event notification to all sender
    /// applications.
    ///
    /// # Arguments
    ///
    /// * `destination` - `protocol` of the media application (e.g. `web-1`);
    /// * `media_session_id` - ID of the media session to be paused.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn pause<S>(&self, destination: S, media_session_id: i32) -> Result<StatusEntry, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.send_pause(destination, media_session_id)?;

        self.wait_for_status(&[request_id], media_session_id)
    }

    /// Begins playback of the content that was loaded with the load call, playback is continued
    /// from the current time position.
    ///
    /// # Arguments
    ///
    /// * `destination` - `protocol` of the media application (e.g. `web-1`);
    /// * `media_session_id` - ID of the media session to be played.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn play<S>(&self, destination: S, media_session_id: i32) -> Result<StatusEntry, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.send_play(destination, media_session_id)?;

        self.wait_for_status(&[request_id], media_session_id)
    }

    /// Stops playback of the current content. Triggers a STATUS event notification to all sender
    /// applications. After this command the content will no longer be loaded and the
    /// media_session_id is invalidated.
    ///
    /// # Arguments
    ///
    /// * `destination` - `protocol` of the media application (e.g. `web-1`);
    /// * `media_session_id` - ID of the media session to be stopped.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn stop<S>(&self, destination: S, media_session_id: i32) -> Result<StatusEntry, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.send_stop(destination, media_session_id)?;

        self.wait_for_status(&[request_id], media_session_id)
    }

    /// Sets the current position in the stream. Triggers a STATUS event notification to all sender
    /// applications. If the position provided is outside the range of valid positions for the
    /// current content, then the player should pick a valid position as close to the requested
    /// position as possible.
    ///
    /// # Arguments
    ///
    /// * `destination` - `protocol` of the media application (e.g. `web-1`);
    /// * `media_session_id` - ID of the media session to seek in;
    /// * `current_time` - Time in seconds to seek to.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn seek<S>(
        &self,
        destination: S,
        media_session_id: i32,
        current_time: Option<f32>,
        resume_state: Option<ResumeState>,
    ) -> Result<StatusEntry, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id =
            self.send_seek(destination, media_session_id, current_time, resume_state)?;

        self.wait_for_status(&[request_id], media_session_id)
    }

    /// Modifies the text tracks style or change the tracks status. If a trackId does not match
    /// the existing trackIds the whole request will fail and no status will change.
    ///
    /// # Arguments
    ///
    /// * `destination` - `protocol` of the media application (e.g. `web-1`);
    /// * `media_session_id` - ID of the media session to modify tracks for;
    /// * `track_selection` - Track selection data.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `Status` instance or an `Error`.
    pub fn edit_tracks<S>(
        &self,
        destination: S,
        media_session_id: i32,
        track_selection: TrackSelection,
    ) -> Result<StatusEntry, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.send_edit_tracks(destination, media_session_id, track_selection)?;

        self.wait_for_status(&[request_id], media_session_id)
    }

    // Send-only variants of the requests above. Each one sends the request without waiting for
    // the reply and returns its request ID, which can then be passed to `wait_for_status`.
    //
    // This allows callers to decide how to wait. For example, some receivers (such as Shaka's)
    // don't echo the request ID of playback commands, but do echo it for GET_STATUS: sending a
    // command followed by a GET_STATUS and waiting for either reply works with both kinds of
    // receivers.

    /// Sends a GET_STATUS request without waiting for the reply. See `get_status`.
    pub fn send_get_status<S>(
        &self,
        destination: S,
        media_session_id: Option<i32>,
    ) -> Result<u32, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.message_manager.generate_request_id().get();

        self.send_request(
            destination,
            &proxies::media::GetStatusRequest {
                typ: MESSAGE_TYPE_GET_STATUS.to_string(),
                request_id,
                media_session_id,
            },
        )?;

        Ok(request_id)
    }

    /// Sends a PAUSE request without waiting for the reply. See `pause`.
    pub fn send_pause<S>(&self, destination: S, media_session_id: i32) -> Result<u32, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        self.send_playback_generic_request(destination, media_session_id, MESSAGE_TYPE_PAUSE)
    }

    /// Sends a PLAY request without waiting for the reply. See `play`.
    pub fn send_play<S>(&self, destination: S, media_session_id: i32) -> Result<u32, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        self.send_playback_generic_request(destination, media_session_id, MESSAGE_TYPE_PLAY)
    }

    /// Sends a STOP request without waiting for the reply. See `stop`.
    pub fn send_stop<S>(&self, destination: S, media_session_id: i32) -> Result<u32, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        self.send_playback_generic_request(destination, media_session_id, MESSAGE_TYPE_STOP)
    }

    /// Sends a SEEK request without waiting for the reply. See `seek`.
    pub fn send_seek<S>(
        &self,
        destination: S,
        media_session_id: i32,
        current_time: Option<f32>,
        resume_state: Option<ResumeState>,
    ) -> Result<u32, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.message_manager.generate_request_id().get();

        self.send_request(
            destination,
            &proxies::media::PlaybackSeekRequest {
                request_id,
                media_session_id,
                typ: MESSAGE_TYPE_SEEK.to_string(),
                current_time,
                resume_state: resume_state.map(|s| s.to_string()),
                custom_data: proxies::media::CustomData::new(),
            },
        )?;

        Ok(request_id)
    }

    /// Sends an EDIT_TRACKS_INFO request without waiting for the reply. See `edit_tracks`.
    pub fn send_edit_tracks<S>(
        &self,
        destination: S,
        media_session_id: i32,
        track_selection: TrackSelection,
    ) -> Result<u32, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.message_manager.generate_request_id().get();

        self.send_request(
            destination,
            &proxies::media::EditTracksInfoRequest {
                request_id,
                media_session_id,
                typ: MESSAGE_TYPE_EDIT_TRACKS_INFO.to_string(),
                active_track_ids: track_selection.active_track_ids,
                enable_text_tracks: track_selection.enable_text_tracks,
                language: track_selection.language,
            },
        )?;

        Ok(request_id)
    }

    pub fn can_handle(&self, message: &CastMessage) -> bool {
        can_handle(message)
    }

    pub fn parse(&self, message: &CastMessage) -> Result<MediaResponse, Error> {
        parse(message)
    }

    /// Waits for the reply to any of the requests in `request_ids`, and returns the status entry
    /// for `media_session_id` it contains.
    ///
    /// Passing several request IDs allows confirming a command with a follow-up GET_STATUS (see
    /// the `send_*` methods): whichever reply arrives first is used, and the others are left in
    /// the message buffer.
    ///
    /// # Arguments
    ///
    /// * `request_ids` - IDs of the requests whose reply should be waited for;
    /// * `media_session_id` - ID of the media session to return the status entry for.
    ///
    /// # Return value
    ///
    /// Returned `Result` should consist of either `StatusEntry` instance or an `Error`. An error is
    /// also returned if the reply does not contain `media_session_id` (e.g. because the media
    /// session has ended).
    pub fn wait_for_status(
        &self,
        request_ids: &[u32],
        media_session_id: i32,
    ) -> Result<StatusEntry, Error> {
        self.message_manager.receive_find_map(|message| {
            if !self.can_handle(message) {
                return Ok(None);
            }

            match self.parse(message)? {
                MediaResponse::Status(mut status) if request_ids.contains(&status.request_id) => {
                    let position = status
                        .entries
                        .iter()
                        .position(|e| e.media_session_id == media_session_id);

                    match position {
                        Some(position) => Ok(Some(status.entries.remove(position))),
                        None => Err(Error::Internal(format!(
                            "Media session {} not found in status reply.",
                            media_session_id
                        ))),
                    }
                }
                MediaResponse::InvalidPlayerState(error)
                    if request_ids.contains(&error.request_id) =>
                {
                    Err(Error::Internal(
                        "Request failed because of invalid player state.".to_string(),
                    ))
                }
                MediaResponse::InvalidRequest(error) if request_ids.contains(&error.request_id) => {
                    Err(Error::Internal(format!(
                        "Invalid request ({}).",
                        error.reason.unwrap_or_else(|| "Unknown".to_string())
                    )))
                }
                _ => Ok(None),
            }
        })
    }

    fn send_playback_generic_request<S>(
        &self,
        destination: S,
        media_session_id: i32,
        typ: &str,
    ) -> Result<u32, Error>
    where
        S: Into<Cow<'a, str>>,
    {
        let request_id = self.message_manager.generate_request_id().get();

        self.send_request(
            destination,
            &proxies::media::PlaybackGenericRequest {
                request_id,
                media_session_id,
                typ: typ.to_string(),
                custom_data: proxies::media::CustomData::new(),
            },
        )?;

        Ok(request_id)
    }

    fn send_request<S, P>(&self, destination: S, payload: &P) -> Result<(), Error>
    where
        S: Into<Cow<'a, str>>,
        P: serde::Serialize,
    {
        self.message_manager.send(CastMessage {
            namespace: CHANNEL_NAMESPACE.to_string(),
            source: self.sender.to_string(),
            destination: destination.into().to_string(),
            payload: CastMessagePayload::String(serde_json::to_string(payload)?),
        })
    }
}

/// Whether `message` belongs to the media channel. Unlike the channel's method, it does not
/// require a channel, e.g. to inspect messages received by an observer.
pub fn can_handle(message: &CastMessage) -> bool {
    message.namespace == CHANNEL_NAMESPACE
}

/// Parses a message of the media channel. Unlike the channel's method, it does not require a
/// channel, e.g. to inspect messages received by an observer.
pub fn parse(message: &CastMessage) -> Result<MediaResponse, Error> {
    let reply = match message.payload {
        CastMessagePayload::String(ref payload) => {
            serde_json::from_str::<serde_json::Value>(payload)?
        }
        _ => {
            return Err(Error::Internal(
                "Binary payload is not supported!".to_string(),
            ));
        }
    };

    let message_type = reply
        .as_object()
        .and_then(|object| object.get("type"))
        .and_then(|property| property.as_str())
        .unwrap_or("")
        .to_string();

    let response = match message_type.as_ref() {
        MESSAGE_TYPE_MEDIA_STATUS => {
            let reply: proxies::media::StatusReply = serde_json::value::from_value(reply)?;

            let statuses_entries = reply.status.iter().map(|x| {
                StatusEntry {
                    active_track_ids: x.active_track_ids.clone(),
                    media_session_id: x.media_session_id,
                    media: x.media.as_ref().map(|m| {
                        Media {
                            content_id: m.content_id.to_string(),
                            stream_type: StreamType::from_str(m.stream_type.as_ref()).unwrap(),
                            content_type: m.content_type.to_string(),
                            metadata: None, // TODO
                            tracks: m
                                .tracks
                                .iter()
                                .map(|t| Track {
                                    id: t.id,
                                    track_type: TrackType::from_str(t.typ.as_ref()).unwrap(),
                                    name: t.name.clone(),
                                    language: t.language.clone(),
                                })
                                .collect(),
                            duration: m.duration,
                        }
                    }),
                    live_seekable_range: x.live_seekable_range.clone(),
                    playback_rate: x.playback_rate,
                    player_state: PlayerState::from_str(x.player_state.as_ref()).unwrap(),
                    idle_reason: x
                        .idle_reason
                        .as_ref()
                        .map(|reason| IdleReason::from_str(reason).unwrap()),
                    current_time: x.current_time,
                    supported_media_commands: x.supported_media_commands,
                }
            });

            MediaResponse::Status(Status {
                request_id: reply.request_id,
                entries: statuses_entries.collect::<Vec<StatusEntry>>(),
            })
        }
        MESSAGE_TYPE_LOAD_CANCELLED => {
            let reply: proxies::media::LoadCancelledReply = serde_json::value::from_value(reply)?;

            MediaResponse::LoadCancelled(LoadCancelled {
                request_id: reply.request_id,
            })
        }
        MESSAGE_TYPE_LOAD_FAILED => {
            let reply: proxies::media::LoadFailedReply = serde_json::value::from_value(reply)?;

            MediaResponse::LoadFailed(LoadFailed {
                request_id: reply.request_id,
            })
        }
        MESSAGE_TYPE_INVALID_PLAYER_STATE => {
            let reply: proxies::media::InvalidPlayerStateReply =
                serde_json::value::from_value(reply)?;

            MediaResponse::InvalidPlayerState(InvalidPlayerState {
                request_id: reply.request_id,
            })
        }
        MESSAGE_TYPE_INVALID_REQUEST => {
            let reply: proxies::media::InvalidRequestReply = serde_json::value::from_value(reply)?;

            MediaResponse::InvalidRequest(InvalidRequest {
                request_id: reply.request_id,
                reason: reply.reason,
            })
        }
        MESSAGE_TYPE_ERROR => {
            let reply: proxies::media::MediaErrorReply = serde_json::value::from_value(reply)?;
            let detailed_error_code = MediaDetailedErrorCode::try_from(reply.detailed_error_code)?;

            MediaResponse::Error(MediaError {
                detailed_error_code,
                message_type: reply.message_type,
            })
        }
        _ => MediaResponse::NotImplemented(message_type.to_string(), reply),
    };

    Ok(response)
}

#[cfg(test)]
mod tests {
    use crate::{
        cast::cast_channel::{
            self,
            cast_message::{PayloadType, ProtocolVersion},
        },
        channels::tests::MockTcpStream,
        utils, DEFAULT_RECEIVER_ID, DEFAULT_SENDER_ID,
    };

    use super::*;

    /// Returns a media channel whose stream will yield the given payloads, in order, as messages
    /// sent by the receiver.
    fn channel_receiving(payloads: &[String]) -> MediaChannel<'static, MockTcpStream> {
        let messages: Vec<_> = payloads
            .iter()
            .map(|payload| (CHANNEL_NAMESPACE, payload.clone()))
            .collect();

        channel_receiving_messages(&messages)
    }

    /// Like `channel_receiving`, with the namespace of each message.
    fn channel_receiving_messages(
        messages: &[(&str, String)],
    ) -> MediaChannel<'static, MockTcpStream> {
        let mut stream = MockTcpStream::new();

        for (namespace, payload) in messages {
            let mut message = cast_channel::CastMessage::new();
            message.set_protocol_version(ProtocolVersion::CASTV2_1_0);
            message.set_namespace(namespace.to_string());
            message.set_source_id(DEFAULT_RECEIVER_ID.to_string());
            message.set_destination_id(DEFAULT_SENDER_ID.to_string());
            message.set_payload_type(PayloadType::STRING);
            message.set_payload_utf8(payload.clone());

            stream
                .read_buffer
                .extend(utils::to_frame(&message).unwrap());
        }

        MediaChannel {
            sender: Cow::from(DEFAULT_SENDER_ID),
            message_manager: Lrc::new(MessageManager::new(stream)),
        }
    }

    fn media_status(request_id: u32, entries: &[(i32, f32)]) -> String {
        let entries = entries
            .iter()
            .map(|(media_session_id, current_time)| {
                format!(
                    "{{\"mediaSessionId\":{},\"playbackRate\":1,\"playerState\":\"PLAYING\",\
                     \"currentTime\":{},\"supportedMediaCommands\":63}}",
                    media_session_id, current_time
                )
            })
            .collect::<Vec<_>>()
            .join(",");

        format!(
            "{{\"type\":\"MEDIA_STATUS\",\"requestId\":{},\"status\":[{}]}}",
            request_id, entries
        )
    }

    #[test]
    fn test_wait_for_status_accepts_reply_to_any_request() {
        // Mimics Shaka: an unsolicited status (requestId 0) and no reply to the command (ID 1),
        // followed by the reply to the follow-up GET_STATUS (ID 2).
        let channel =
            channel_receiving(&[media_status(0, &[(0, 10.0)]), media_status(2, &[(0, 42.0)])]);

        let entry = channel.wait_for_status(&[1, 2], 0).unwrap();

        assert_eq!(entry.current_time, Some(42.0));
    }

    #[test]
    fn test_wait_for_status_picks_requested_media_session() {
        let channel = channel_receiving(&[media_status(1, &[(3, 10.0), (7, 42.0)])]);

        let entry = channel.wait_for_status(&[1], 7).unwrap();

        assert_eq!(entry.media_session_id, 7);
        assert_eq!(entry.current_time, Some(42.0));
    }

    #[test]
    fn test_wait_for_status_fails_when_media_session_is_missing() {
        // Without this, the wait would continue until the connection is closed.
        let channel = channel_receiving(&[media_status(1, &[]), media_status(2, &[(0, 42.0)])]);

        assert!(channel.wait_for_status(&[1, 2], 0).is_err());
    }

    #[test]
    fn test_wait_for_status_returns_reply_received_after_deadline() {
        let channel = channel_receiving(&[media_status(1, &[(0, 42.0)])]);
        channel
            .message_manager
            .set_deadline(Some(std::time::Instant::now()));

        assert!(channel.wait_for_status(&[1], 0).is_ok());
    }

    #[test]
    fn test_wait_for_status_fails_after_deadline() {
        let channel =
            channel_receiving(&[media_status(0, &[(0, 10.0)]), media_status(1, &[(0, 42.0)])]);
        channel
            .message_manager
            .set_deadline(Some(std::time::Instant::now()));

        match channel.wait_for_status(&[1], 0) {
            Err(Error::Io(e)) => assert_eq!(e.kind(), std::io::ErrorKind::TimedOut),
            other => panic!("expected a timeout, got {:?}", other.map(|_| ())),
        }
    }

    const PING: (&str, &str) = (
        "urn:x-cast:com.google.cast.tp.heartbeat",
        r#"{"type":"PING"}"#,
    );

    #[test]
    fn test_wait_for_status_answers_pings() {
        let channel = channel_receiving_messages(&[
            (PING.0, PING.1.to_string()),
            (CHANNEL_NAMESPACE, media_status(1, &[(0, 42.0)])),
        ]);
        channel.message_manager.set_answer_pings(true);

        assert!(channel.wait_for_status(&[1], 0).is_ok());
        // the ping was answered rather than kept for later.
        assert!(channel.message_manager.drain().is_empty());
    }

    #[test]
    fn test_wait_for_status_keeps_pings_by_default() {
        let channel = channel_receiving_messages(&[
            (PING.0, PING.1.to_string()),
            (CHANNEL_NAMESPACE, media_status(1, &[(0, 42.0)])),
        ]);

        assert!(channel.wait_for_status(&[1], 0).is_ok());
        assert_eq!(channel.message_manager.drain().len(), 1);
    }

    #[test]
    fn test_wait_for_status_fails_on_invalid_player_state() {
        let channel = channel_receiving(&[
            "{\"type\":\"INVALID_PLAYER_STATE\",\"requestId\":5}".to_string(),
            "{\"type\":\"INVALID_PLAYER_STATE\",\"requestId\":1}".to_string(),
            media_status(2, &[(0, 42.0)]),
        ]);

        // the error for request 5 is not ours and must be skipped; the one for request 1 is.
        assert!(channel.wait_for_status(&[1, 2], 0).is_err());
    }

    #[test]
    fn test_parse_media_error() {
        let message = CastMessage {
            namespace: CHANNEL_NAMESPACE.to_string(),
            source: DEFAULT_RECEIVER_ID.to_string(),
            destination: DEFAULT_SENDER_ID.to_string(),
            payload: CastMessagePayload::String(
                "{\"type\":\"ERROR\",\"detailedErrorCode\":104,\"itemId\":1}".to_string(),
            ),
        };
        let channel = MediaChannel {
            sender: Cow::from(DEFAULT_SENDER_ID),
            message_manager: Lrc::new(MessageManager::new(MockTcpStream::new())),
        };
        let expected_result = MediaError {
            detailed_error_code: MediaDetailedErrorCode::MediaSrcNotSupported,
            message_type: MESSAGE_TYPE_ERROR.to_string(),
        };

        let response = channel.parse(&message).unwrap();

        if let MediaResponse::Error(result) = response {
            assert_eq!(expected_result, result);
        } else {
            panic!("expected MediaResponse::Error, but got {:?}", response);
        }
    }

    #[test]
    fn test_parse_unknown_message_type() {
        let message_type = "FOO_BAR";
        let payload = format!("{{\"type\":\"{}\",\"itemId\":666}}", message_type);
        let expected_payload = serde_json::from_str::<serde_json::Value>(payload.as_str()).unwrap();
        let message = CastMessage {
            namespace: CHANNEL_NAMESPACE.to_string(),
            source: DEFAULT_RECEIVER_ID.to_string(),
            destination: DEFAULT_SENDER_ID.to_string(),
            payload: CastMessagePayload::String(payload),
        };
        let channel = MediaChannel {
            sender: Cow::from(DEFAULT_SENDER_ID),
            message_manager: Lrc::new(MessageManager::new(MockTcpStream::new())),
        };

        let response = channel.parse(&message).unwrap();

        if let MediaResponse::NotImplemented(result_code, result_payload) = response {
            assert_eq!(message_type, result_code);
            assert_eq!(expected_payload, result_payload);
        } else {
            panic!("expected MediaResponse::Error, but got {:?}", response);
        }
    }
}
