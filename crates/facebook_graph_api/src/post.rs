#[cfg(feature = "axum")]
use axum::{http::StatusCode, response::IntoResponse};
use serde::{Deserialize, Serialize};
use std::{
    ops::Deref,
    time::{SystemTime, UNIX_EPOCH},
};
#[cfg(feature = "time")]
use time::{OffsetDateTime, format_description::well_known::Iso8601};
use unicode_segmentation::UnicodeSegmentation;
use url::Url;

use crate::Error;

const MIN_DELAY_SECS: i64 = 600; // 10 Minutes
const MAX_DELAY_SECS: i64 = 2_592_000; // 30 Days
const MIN_CHARS: u64 = 10;
const MAX_CHARS: u64 = 24_000;

/// An effort to enforce the invariants explained in the Page API docs for a scheduled publish time
/// for a post
/// `<https://developers.facebook.com/docs/pages-api/posts#publish_posts>`
#[derive(Debug, PartialEq)]
pub struct ScheduledTime(String);

impl ScheduledTime {
    /// Validation occurs at construction time; the API call may still fail if delayed.
    /// Enforces Facebook Page API scheduling invariants.
    fn check_timestamp(timestamp: i64) -> Result<(), ScheduledTimeError> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("Somehow the system time is set before the unix epoch")
            .as_secs()
            .cast_signed();

        let diff = timestamp - now;
        if diff < 0 {
            Err(ScheduledTimeError::ScheduleInPast)
        } else if diff < MIN_DELAY_SECS {
            Err(ScheduledTimeError::ScheduleTooSoon)
        } else if diff > MAX_DELAY_SECS {
            Err(ScheduledTimeError::ScheduleTooFar)
        } else {
            Ok(())
        }
    }

    // TODO: Write the bindings for timelib.c so I can replicate the behaviour of PHP's strtotime
    // class, which is what the Page API uses.
    fn validate_scheduled_time(s: &str) -> Result<(), ScheduledTimeError> {
        // Try unix timestamp (seconds)
        if let Ok(ts) = s.parse::<i64>() {
            return Self::check_timestamp(ts);
        }

        // Try ISO-8601 timestamp
        #[cfg(feature = "time")]
        {
            match OffsetDateTime::parse(s, &Iso8601::DEFAULT) {
                Ok(dt) => return Self::check_timestamp(dt.unix_timestamp()),
                Err(_) => {
                    return Err(ScheduledTimeError::invalid_iso_str());
                }
            }
        }

        #[cfg(not(feature = "time"))]
        Err(ScheduledTimeError::invalid_timestamp())
    }
    /// Parse and validate a scheduled publish time
    pub fn parse(s: &str) -> Result<Self, ScheduledTimeError> {
        Self::validate_scheduled_time(s)?;
        Ok(Self(s.to_owned()))
    }
}

impl Deref for ScheduledTime {
    type Target = String;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[cfg(feature = "time")]
impl TryFrom<ScheduledTime> for OffsetDateTime {
    type Error = ScheduledTimeError;

    fn try_from(value: ScheduledTime) -> Result<Self, Self::Error> {
        // unix timestamp
        if let Ok(ts) = value.0.parse::<i64>() {
            return OffsetDateTime::from_unix_timestamp(ts)
                .map_err(|_| ScheduledTimeError::invalid_iso_str());
        }

        // ISO-8601
        OffsetDateTime::parse(&value.0, &Iso8601::DEFAULT)
            .map_err(|_| ScheduledTimeError::invalid_iso_str())
    }
}

#[cfg(feature = "time")]
impl TryFrom<&ScheduledTime> for OffsetDateTime {
    type Error = ScheduledTimeError;

    fn try_from(value: &ScheduledTime) -> Result<Self, Self::Error> {
        // unix timestamp
        if let Ok(ts) = value.0.parse::<i64>() {
            return OffsetDateTime::from_unix_timestamp(ts)
                .map_err(|_| ScheduledTimeError::invalid_iso_str());
        }

        // ISO-8601
        OffsetDateTime::parse(&value.0, &Iso8601::DEFAULT)
            .map_err(|_| ScheduledTimeError::invalid_iso_str())
    }
}

/// Media input can be a URL, base64 data, or a reference to uploaded file
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub enum Input {
    /// URL to an image
    Url(String),
    /// Base64 encoded image with optional metadata
    Base64 {
        data: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        filename: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        content_type: Option<String>,
    },
}

/// An effort to enforce the invariants explained in the Page API docs for the parameters needed to
/// do a post in a Facebook Page via the Page API
/// `<https://developers.facebook.com/docs/pages-api/posts#publish_posts>`
#[derive(Debug)]
pub struct FacebookPost {
    pub message: String,
    pub published: bool,
    pub link: Option<String>,
    pub scheduled_publish_time: Option<ScheduledTime>,
    pub media_url: Option<Vec<Input>>,
}

impl FacebookPost {
    pub fn new(
        message: String,
        scheduled_publish_time: Option<String>,
        link: Option<String>,
        media: Option<Vec<Input>>,
    ) -> Result<Self, Error> {
        Self::validate_content(&message)?;

        if let Some(ref url) = link {
            Url::parse(url).map_err(PostError::from)?;
        }

        // This way I enforce the invariant of published being false if there is a
        // scheduled_publish_time passed to the Page API post endpoint. published = true would
        // means that the post has to be immediately published.
        let (published, scheduled_publish_time) = {
            match scheduled_publish_time {
                None => (true, None),
                Some(ts) => {
                    let schedule_time = ScheduledTime::parse(&ts).map_err(PostError::from)?;

                    (false, Some(schedule_time))
                }
            }
        };

        Ok(Self {
            message,
            published,
            link,
            scheduled_publish_time,
            media_url: media,
        })
    }

    /// An effort to enforce the invariants explained in the Page API docs for the message in a post
    /// `<https://developers.facebook.com/docs/pages-api/posts#publish_posts>`
    fn validate_content(s: &str) -> Result<(), PostError> {
        let graphemes = s.graphemes(true).count() as u64;

        if s.trim().is_empty() {
            return Err(PostError::MessageError(
                "Message is empty or is entirely whitespace".to_string(),
            ));
        }

        if graphemes > MAX_CHARS {
            return Err(PostError::MessageError(
                "Message is too long. The maximum is 24000 characters".to_string(),
            ));
        }

        if graphemes < MIN_CHARS {
            return Err(PostError::MessageError(
                "Message is too short. The minimum is 10 characters".to_string(),
            ));
        }

        Ok(())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum PostError {
    #[error(transparent)]
    ScheduleError(#[from] ScheduledTimeError),
    #[error(transparent)]
    LinkError(#[from] url::ParseError),
    #[error("{0}")]
    MessageError(String),
}

#[derive(Debug, thiserror::Error)]
pub enum ScheduledTimeError {
    #[error("Scheduled publish time must be at least 10 minutes in the future")]
    ScheduleTooSoon,
    #[error("Scheduled publish time must be at most 30 days in the future")]
    ScheduleTooFar,
    #[error("Scheduled publish time is in the past")]
    ScheduleInPast,
    #[error("Invalid format. \"{0}\"")]
    Invalid(String),
}

/// TODO: Think about a better way of encoding this type of errors
impl ScheduledTimeError {
    #[cfg(feature = "time")]
    pub fn invalid_iso_str() -> ScheduledTimeError {
        ScheduledTimeError::Invalid("The time str isn't compliant to ISO-8601".to_string())
    }

    pub fn invalid_timestamp() -> ScheduledTimeError {
        ScheduledTimeError::Invalid(
            "The time str isn't in UNIX timestamp representation".to_string(),
        )
    }
}

#[cfg(feature = "axum")]
impl IntoResponse for PostError {
    fn into_response(self) -> axum::response::Response {
        use axum::Json;
        use serde_json::json;

        let (status, message) = match self {
            PostError::ScheduleError(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            PostError::LinkError(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            PostError::MessageError(msg) => (StatusCode::BAD_REQUEST, msg),
        };

        let body = Json(json!({
            "error": message,
        }));
        (status, body).into_response()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use expect_test::{Expect, expect};
    use pretty_assertions::assert_eq;
    use quickcheck::TestResult;
    use quickcheck_macros::quickcheck;

    fn check<T: std::fmt::Debug>(body: T, expect: Expect) {
        expect.assert_debug_eq(&body);
    }

    #[test]
    fn valid_immediate_post_is_created() {
        let post = FacebookPost::new(
            "This is a valid message with enough characters!".to_string(),
            None,
            None,
        );

        assert!(post.is_ok());
        let post = post.unwrap();
        assert!(post.published);
        assert!(post.scheduled_publish_time.is_none());
    }

    // #[test]
    // fn valid_scheduled_post_is_created() {
    //     let now = SystemTime::now()
    //         .duration_since(UNIX_EPOCH)
    //         .unwrap()
    //         .as_secs() as i64;
    //     let future = now + 3600;

    //     let post = FacebookPost::new(
    //         "Valid scheduled post!".to_string(),
    //         Some(future.to_string()),
    //         None,
    //     );

    //     assert!(post.is_ok());
    //     let post = post.unwrap();
    //     assert!(!post.published);
    //     assert_eq!(post.scheduled_publish_time, Some(future.to_string()));
    // }

    #[test]
    fn valid_post_with_link_is_created() {
        let post = FacebookPost::new(
            "Check out this link!".to_string(),
            None,
            Some("https://example.com".to_string()),
        );

        assert!(post.is_ok());
        let post = post.unwrap();
        assert_eq!(post.link, Some("https://example.com".to_string()));
    }

    #[test]
    fn content_accepts_exactly_min_chars() {
        let s = "1234567890".to_string();
        let post = FacebookPost::new(s, None, None);
        assert!(post.is_ok());
    }

    #[test]
    fn content_accepts_exactly_max_chars() {
        let s = "a".repeat(24_000);
        let post = FacebookPost::new(s, None, None);
        assert!(post.is_ok());
    }

    #[test]
    fn scheduled_time_exactly_10_minutes_is_accepted() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let time = now + MIN_DELAY_SECS;
        let post = FacebookPost::new(
            "Valid message here".to_string(),
            Some(time.to_string()),
            None,
        );

        assert!(post.is_ok());
    }

    #[test]
    fn scheduled_time_exactly_30_days_is_accepted() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let time = now + MAX_DELAY_SECS;
        let post = FacebookPost::new(
            "Valid message here".to_string(),
            Some(time.to_string()),
            None,
        );

        assert!(post.is_ok());
    }

    // TODO: Write the bindings for timelib.c so I can replicate the behaviour of PHP's strtotime
    // class, which is what the Page API uses.
    // #[test]
    // fn strtotime_compatible_strings_are_accepted() {
    //     let post = FacebookPost::new(
    //         "Valid message here".to_string(),
    //         Some("+2 weeks".to_string()),
    //         None,
    //     );

    //     assert!(post.is_ok());
    // }

    #[test]
    fn content_rejects_whitespace_only() {
        let post = FacebookPost::new("     \n\t   ".to_string(), None, None);

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        MessageError(
                            "Message is empty or is entirely whitespace",
                        ),
                    ),
                )
            "#]],
        );
    }

    #[test]
    fn content_rejects_too_few_graphemes() {
        let s = "👍👍👍👍👍👍👍👍👍".to_string();

        let post = FacebookPost::new(s, None, None);

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        MessageError(
                            "Message is too short. The minimum is 10 characters",
                        ),
                    ),
                )
            "#]],
        );
    }

    #[test]
    fn content_rejects_too_many_graphemes() {
        let s = "👍".repeat(24_001);

        let post = FacebookPost::new(s, None, None);

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        MessageError(
                            "Message is too long. The maximum is 24000 characters",
                        ),
                    ),
                )
            "#]],
        );
    }

    #[test]
    fn scheduled_time_too_far_is_rejected() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let time = now + MAX_DELAY_SECS + 1;
        let post = FacebookPost::new(
            "Valid message here".to_string(),
            Some(time.to_string()),
            None,
        );

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        ScheduleError(
                            ScheduleTooFar,
                        ),
                    ),
                )
            "#]],
        );
    }

    #[test]
    fn scheduled_time_too_soon_is_rejected() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let time = now + 60;
        let post = FacebookPost::new(
            "Valid message here".to_string(),
            Some(time.to_string()),
            None,
        );

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        ScheduleError(
                            ScheduleTooSoon,
                        ),
                    ),
                )
            "#]],
        );
    }

    #[test]
    fn scheduled_time_in_the_past_is_rejected() {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let time = now - 3600;
        let post = FacebookPost::new(
            "Valid message here".to_string(),
            Some(time.to_string()),
            None,
        );

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        ScheduleError(
                            ScheduleInPast,
                        ),
                    ),
                )
            "#]],
        );
    }

    #[test]
    fn link_rejects_invalid_url() {
        let post = FacebookPost::new(
            "This is a valid message content".to_string(),
            None,
            Some("not a url!!!".to_string()),
        );

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        LinkError(
                            RelativeUrlWithoutBase,
                        ),
                    ),
                )
            "#]],
        );
    }

    #[test]
    fn link_rejects_relative_url() {
        let post = FacebookPost::new(
            "This is a valid message content".to_string(),
            None,
            Some("/relative/path".to_string()),
        );

        check(
            post,
            expect![[r#"
                Err(
                    PostError(
                        LinkError(
                            RelativeUrlWithoutBase,
                        ),
                    ),
                )
            "#]],
        );
    }

    #[quickcheck]
    fn message_length_is_consistent(s: String) -> TestResult {
        if s.trim().is_empty() {
            return TestResult::discard();
        }

        let grapheme_count = s.graphemes(true).count() as u64;
        let result = FacebookPost::validate_content(&s);

        match (grapheme_count, result) {
            (MIN_CHARS..=MAX_CHARS, Ok(())) => TestResult::passed(),
            (n, Err(PostError::MessageError(_))) if n < MIN_CHARS || n > MAX_CHARS => {
                TestResult::passed()
            }
            _ => TestResult::failed(),
        }
    }
    // #[test]
    // fn timestamp_boundaries_are_exact() {
    //     let now = SystemTime::now()
    //         .duration_since(UNIX_EPOCH)
    //         .unwrap()
    //         .as_secs() as i64;

    //     let at_min = FacebookPost::check_timestamp(now + MIN_DELAY_SECS);
    //     let before_min = FacebookPost::check_timestamp(now + MIN_DELAY_SECS - 1);
    //     let at_max = FacebookPost::check_timestamp(now + MAX_DELAY_SECS);
    //     let after_max = FacebookPost::check_timestamp(now + MAX_DELAY_SECS + 1);

    //     assert!(at_min.is_ok());
    //     assert!(matches!(
    //         before_min,
    //         Err(ScheduledTimeError::ScheduleTooSoon)
    //     ));
    //     assert!(at_max.is_ok());
    //     assert!(matches!(after_max, Err(ScheduledTimeError::ScheduleTooFar)));
    // }

    #[test]
    fn character_boundaries_are_exact() {
        let at_min = "a".repeat(MIN_CHARS as usize);
        let at_min_result = FacebookPost::validate_content(&at_min);

        let before_min = "a".repeat((MIN_CHARS - 1) as usize);
        let before_min_result = FacebookPost::validate_content(&before_min);

        let at_max = "a".repeat(MAX_CHARS as usize);
        let at_max_result = FacebookPost::validate_content(&at_max);

        let after_max = "a".repeat((MAX_CHARS + 1) as usize);
        let after_max_result = FacebookPost::validate_content(&after_max);

        assert!(at_min_result.is_ok());
        assert!(before_min_result.is_err());
        assert!(at_max_result.is_ok());
        assert!(after_max_result.is_err());
    }

    #[quickcheck]
    fn scheduled_posts_are_never_published(message: String, timestamp_offset: i64) -> TestResult {
        let grapheme_count = message.graphemes(true).count() as u64;
        if message.trim().is_empty() || grapheme_count < MIN_CHARS || grapheme_count > MAX_CHARS {
            return TestResult::discard();
        }

        if timestamp_offset < MIN_DELAY_SECS || timestamp_offset > MAX_DELAY_SECS {
            return TestResult::discard();
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as i64;

        let timestamp = now + timestamp_offset;

        if let Ok(post) = FacebookPost::new(message, Some(timestamp.to_string()), None) {
            TestResult::from_bool(!post.published && post.scheduled_publish_time.is_some())
        } else {
            TestResult::failed()
        }
    }

    #[quickcheck]
    fn immediate_posts_are_always_published(message: String) -> TestResult {
        let grapheme_count = message.graphemes(true).count() as u64;
        if message.trim().is_empty() || grapheme_count < MIN_CHARS || grapheme_count > MAX_CHARS {
            return TestResult::discard();
        }

        if let Ok(post) = FacebookPost::new(message, None, None) {
            TestResult::from_bool(post.published && post.scheduled_publish_time.is_none())
        } else {
            TestResult::failed()
        }
    }

    // #[quickcheck]
    // fn url_validation_is_consistent(message: String, url: String) -> TestResult {
    //     let grapheme_count = message.graphemes(true).count() as u64;
    //     if message.trim().is_empty() || grapheme_count < MIN_CHARS || grapheme_count > MAX_CHARS {
    //         return TestResult::discard();
    //     }

    //     let url_parse_result = Url::parse(&url);
    //     let post_result = FacebookPost::new(message, None, Some(url));

    //     match (url_parse_result, post_result) {
    //         (Ok(_), Ok(_)) => TestResult::passed(),
    //         (Err(_), Err(PostError::LinkError(_))) => TestResult::passed(),
    //         _ => TestResult::failed(),
    //     }
    // }

    #[quickcheck]
    fn post_creation_is_deterministic(
        message: String,
        timestamp_offset: Option<i64>,
        url: Option<String>,
    ) -> TestResult {
        let grapheme_count = message.graphemes(true).count() as u64;
        if message.trim().is_empty() || grapheme_count < MIN_CHARS || grapheme_count > MAX_CHARS {
            return TestResult::discard();
        }

        let timestamp_str = if let Some(offset) = timestamp_offset {
            if offset < MIN_DELAY_SECS || offset > MAX_DELAY_SECS {
                return TestResult::discard();
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            Some((now + offset).to_string())
        } else {
            None
        };

        if let Some(ref u) = url {
            if Url::parse(u).is_err() {
                return TestResult::discard();
            }
        }

        let result1 = FacebookPost::new(message.clone(), timestamp_str.clone(), url.clone());
        let result2 = FacebookPost::new(message, timestamp_str, url);

        match (result1, result2) {
            (Ok(post1), Ok(post2)) => TestResult::from_bool(
                post1.message == post2.message
                    && post1.published == post2.published
                    && post1.link == post2.link
                    && post1.scheduled_publish_time == post2.scheduled_publish_time,
            ),
            (Err(_), Err(_)) => TestResult::passed(),
            _ => TestResult::failed(),
        }
    }

    #[quickcheck]
    fn message_validation_is_independent(
        message: String,
        timestamp_offset: Option<i64>,
        url: Option<String>,
    ) -> TestResult {
        let grapheme_count = message.graphemes(true).count() as u64;
        let message_is_valid = !message.trim().is_empty()
            && grapheme_count >= MIN_CHARS
            && grapheme_count <= MAX_CHARS;

        let timestamp_str = if let Some(offset) = timestamp_offset {
            if offset < MIN_DELAY_SECS || offset > MAX_DELAY_SECS {
                return TestResult::discard();
            }
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64;
            Some((now + offset).to_string())
        } else {
            None
        };

        if let Some(ref u) = url {
            if Url::parse(u).is_err() {
                return TestResult::discard();
            }
        }

        let result = FacebookPost::new(message, timestamp_str, url);

        match (message_is_valid, result) {
            (true, Ok(_)) => TestResult::passed(),
            (false, Err(crate::Error::PostError(_))) => TestResult::passed(),
            _ => TestResult::failed(),
        }
    }

    #[quickcheck]
    fn grapheme_counting_handles_unicode(s: String) -> bool {
        let grapheme_count = s.graphemes(true).count();
        grapheme_count <= s.len() && grapheme_count <= s.chars().count()
    }
}
