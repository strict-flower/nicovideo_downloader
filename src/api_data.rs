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
    pub _9d091f87: Option<bool>,
    pub acf68865: bool,
    pub count: VideoCount,
    pub duration: i32,
    pub id: String,
    pub isChannelVideo: bool,
    pub isMuted: bool,
    pub isPaymentRequired: bool,
    pub latestCommentSummary: String,
    pub owner: VideoOwner,
    pub playbackPosition: Option<f64>,
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
    pub _9d091f87: Option<bool>,
    pub count: VideoCount,
    pub duration: i32,
    pub description: String,
    pub id: String,
    pub isPrivate: Option<bool>,
    pub isDeleted: Option<bool>,
    pub isNoBanner: Option<bool>,
    pub isAuthenticationRequired: Option<bool>,
    pub isEmbedPlayerAllowed: Option<bool>,
    pub isGiftAllowed: Option<bool>,
    pub rating: Option<VideoRating>,
    pub registeredAt: String,
    pub thumbnail: VideoThumbnail,
    pub title: String,
    pub viewer: Option<VideoViewer>,
    pub watchableUserTypeForPayment: Option<String>,
    pub commentableUserTypeForPayment: Option<String>,
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

    StoryBoardImage {
        id: String
    }

    StoryBoard {
        contentId: String,
        images: Vec<StoryBoardImage>,
        session: Session,

    }

    DomandVideo {
        id: String,
        isAvailable: bool,
        label: String,
        bitRate: i32,
        width: i32,
        height: i32,
        qualityLevel: i32,
        recommendedHighestAudioQualityLevel: i32
    }

    DomandAudio {
        id: String,
        isAvailable: bool,
        bitRate: i32,
        samplingRate: i32,
        integratedLoudness: f64,
        truePeak: f64,
        qualityLevel: i32,
        loudnessCollection: Vec<TypewiseLoudness>,
    }

    Domand {
        videos: Vec<DomandVideo>,
        audios: Vec<DomandAudio>,
        isStoryboardAvailable: bool,
        accessRightKey: String
    }

    Media {
        domand: Domand,
        delivery: Value,
        deliveryLegacy: Option<Value>
    }

    VideoCount {
        view: i32,
        comment: i32,
        mylist: i32,
        like: i32
    }

    VideoOwner {
        iconUrl: Option<String>,
        id: String,
        name: Option<String>,
        ownerType: String,
        r#type: String,
        visibility: String
    }

    VideoThumbnail {
        url: String
        middleUrl: Option<String>,
        largeUrl: Option<String>,
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
        client: Client,
        comment: Option<Value>,
        community: Option<Value>,
        easyComment: Option<Value>,
        external: Option<Value>,
        genre: Option<Value>,
        marquee: Option<Value>,
        media: Media,
        okReason: Option<String>,
        owner: Option<Value>,
        payment: Option<Value>,
        pcWatchPage: Option<Value>,
        player: Option<Value>,
        ppv: Option<Value>,
        ranking: Option<Value>,
        series: Option<Series>,
        smartphone: Option<Value>,
        system: Option<Value>,
        tag: Value,
        video: Video,
        videoAds: Option<Value>,
        videoLive: Option<Value>,
        viewer: Option<Value>,
        waku: Option<Value>,
    }
}
