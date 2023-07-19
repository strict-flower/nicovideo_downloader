use serde::{Deserialize, Serialize};
use serde_json::Value;

macro_rules! adstruct {
    ($($name:ident { $($stname:ident $stcolon:tt $sttype:ty $(,)?)* }) *) => {
        $(
            #[derive(Serialize, Deserialize, Debug)]
            #[allow(non_snake_case)]
            pub struct $name {
                $(pub $stname $stcolon $sttype,)*
            }
        )*
    }
}

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct SeriesVideo {
    #[serde(rename = "9d091f87")]
    pub _9d091f87: bool,
    pub acf68865: bool,
    pub count: VideoCount,
    pub duration: i32,
    pub id: String,
    pub isChannelVideo: bool,
    pub isMuted: bool,
    pub isPaymentRequired: bool,
    pub latestCommentSummary: String,
    pub owner: VideoOwner,
    pub playbackPosition: i32,
    pub registeredAt: String,
    pub requireSensitiveMasking: bool,
    pub shortDescription: String,
    pub thumbnail: VideoThumbnail,
    pub title: String,
    pub r#type: String,
    pub videoLive: Option<Value>,
}

#[derive(Serialize, Deserialize, Debug)]
#[allow(non_snake_case)]
pub struct Video {
    #[serde(rename = "9d091f87")]
    pub _9d091f87: bool,
    pub count: VideoCount,
    pub duration: i32,
    pub id: String,
    pub isPrivate: bool,
    pub isDeleted: bool,
    pub isNoBanner: bool,
    pub isAuthenticationRequired: bool,
    pub isEmbedPlayerAllowed: bool,
    pub isGiftAllowed: bool,
    pub rating: VideoRating,
    pub registeredAt: String,
    pub thumbnail: VideoThumbnail,
    pub title: String,
    pub viewer: VideoViewer,
    pub watchableUserTypeForPayment: String,
    pub commentableUserTypeForPayment: String,
}

adstruct! {
    Client {
        nicosid: String,
        watchId: String,
        watchTrackId: String,
    }

    AudioLoudness {
        integratedLoudness: f64,
        truePeak: f64,
    }

    TypewiseLoudness {
        r#type: String,
        value: f64,
    }

    AudioMetadata {
        bitrate: i32,
        samplingRate: i32,
        loudness: AudioLoudness,
        levelIndex: i32,
        loudnessCollection: Vec<TypewiseLoudness>,
    }

    MovieAudio {
        id: String,
        isAvailable: bool,
        metadata: AudioMetadata,
    }

    VideoResolution {
        width: i32,
        height: i32,
    }

    VideoMetadata {
        label: String,
        bitrate: i32,
        resolution: VideoResolution,
        levelIndex: i32,
        recommendedHighestAudioLevelIndex: i32
    }

    MovieVideo {
        id: String,
        isAvailable: bool,
        metadata: VideoMetadata
    }

    SessionURL {
        url: String,
        isWellKnownPort: bool,
        isSsl: bool
    }

    Session {
        recipeId: String,
        playerId: String,
        videos: Vec<String>,
        audios: Vec<String>,
        movies: Vec<String>,
        protocols: Vec<String>,
        authTypes: Value,
        serviceUserId: String,
        token: String,
        signature: String,
        contentId: String,
        heartbeatLifetime: i32,
        contentKeyTimeout: i32,
        priority: f32,
        transferPresets: Vec<String>,
        urls: Vec<SessionURL>
    }

    MediaDelivaryMovie {
        contentId: String,
        audios: Vec<MovieAudio>,
        videos: Vec<MovieVideo>,
        session: Session,
    }

    StoryBoardImage {
        id: String
    }

    StoryBoard {
        contentId: String,
        images: Vec<StoryBoardImage>,
        session: Session,

    }

    MediaDelivary {
        recipeId: String,
        encryption: Option<Value>,
        movie: MediaDelivaryMovie,
        storyboard: StoryBoard,
        trackingId: String,
    }

    Media {
        delivery: MediaDelivary,
        deliveryLegacy: Option<Value>
    }

    VideoCount {
        view: i32,
        comment: i32,
        mylist: i32,
        like: i32
    }

    VideoOwner {
        iconUrl: String,
        id: String,
        name: String,
        ownerType: String,
        r#type: String,
        visibility: String
    }

    VideoThumbnail {
        url: String
        middleUrl: String,
        largeUrl: String,
        listingUrl: Option<Value>,
        nHdUrl: Option<Value>,
        player: Option<Value>,
        ogp: Option<Value>,
    }

    SeriesVideos {
        first: SeriesVideo,
        next: Option<SeriesVideo>,
        prev: Option<SeriesVideo>
    }

    Series {
        description: String,
        id: i32,
        thumbnailUrl: String,
        title: String,
        video: SeriesVideos
    }

    VideoRating {
        isAdult: bool
    }

    VideoViewerLike {
        isLiked: bool,
        count: Option<Value>
    }

    VideoViewer {
        isOwner: bool,
        like: VideoViewerLike
    }

    ApiData {
        ads: Option<Value>,
        category: Option<Value>,
        channel: Option<Value>,
        client: Value,
        comment: Value,
        community: Option<Value>,
        easyComment: Value,
        external: Value,
        genre: Value,
        marquee: Value,
        media: Media,
        okReason: String,
        owner: Value,
        payment: Value,
        pcWatchPage: Value,
        player: Value,
        ppv: Option<Value>,
        ranking: Value,
        series: Option<Series>,
        smartphone: Option<Value>,
        system: Value,
        tag: Value,
        video: Video,
        videoAds: Value,
        videoLive: Option<Value>,
        viewer: Value,
        waku: Value,
    }
}
