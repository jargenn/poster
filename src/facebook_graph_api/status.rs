use std::{
    error::Error,
    fmt::{self},
    num::{NonZeroU16, NonZeroU32},
};
/// An implementation of the errors in the Graph API.
/// Where they are represented by a combination of a _code_ _sub-code_ pairing.
///
/// As this crate is not yet meant to be public, this doesnt aim to have a full test suite, as the
/// crate doesn't need it.
///
/// As Facebook errors are identified by **both** _code_ and _subcode_, one needs both to
/// represent a status, where it is said subcode could be missing from responses.
///
/// Read more: https://developers.facebook.com/docs/graph-api/guides/error-handling?locale=en_US#errorcodes
///
/// # Examples
/// ```
/// use poster::facebook_graph_api::FbStatusCode;
///
/// assert_eq!(FbStatusCode::from_parts(190, Some(463)).unwrap(), FbStatusCode::TOKEN_EXPIRED);
/// assert_eq!(FbStatusCode::INVALID_SESSION.as_parts(), (190,492));
///
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct FbStatusCode {
    code: NonZeroU32,
    subcode: Option<NonZeroU16>,
}
/// A possible error value when converting a `FbStatusCode` from a parts.
///
pub struct InvalidStatusCode {
    _priv: (),
}

impl FbStatusCode {
    /// This functions validates the correctness of the supplied subcode. The ErrorCodes defined in
    /// the facebook docs don't have strong invariants so the correctness check is loosed.
    pub const fn from_parts(
        code: u32,
        subcode: Option<u16>,
    ) -> Result<FbStatusCode, InvalidStatusCode> {
        // TODO: Figure out good invariants to enforce for sanity
        let code = match NonZeroU32::new(code) {
            Some(c) => c,
            None => return Err(InvalidStatusCode::new()),
        };

        let subcode = match subcode {
            None => None,
            Some(0) => None,
            Some(v) => match NonZeroU16::new(v) {
                Some(v) => Some(v),
                None => return Err(InvalidStatusCode::new()),
            },
        };

        Ok(Self { code, subcode })
    }

    /// Returns (code, subcode) where subcode will be 0 if not present.
    pub fn as_parts(&self) -> (u32, u16) {
        (self.code.get(), self.subcode.map(|v| v.get()).unwrap_or(0))
    }

    pub fn canonical_reason(&self) -> Option<&'static str> {
        canonical_reason(self.code.into(), self.subcode.map(NonZeroU16::get))
    }
}

impl fmt::Debug for FbStatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FbStatusCode({}, {:?})",
            self.code.get(),
            self.subcode.map(|v| v.get())
        )
    }
}

impl PartialEq<FbStatusCode> for (u32, u16) {
    fn eq(&self, other: &FbStatusCode) -> bool {
        *self == other.as_parts()
    }
}

impl PartialEq<(u32, u16)> for FbStatusCode {
    fn eq(&self, other: &(u32, u16)) -> bool {
        self.as_parts() == *other
    }
}

impl TryFrom<(u32, u16)> for FbStatusCode {
    type Error = InvalidStatusCode;

    fn try_from(t: (u32, u16)) -> Result<Self, Self::Error> {
        let subcode = match t.1 {
            0 => None,
            v => Some(v),
        };
        FbStatusCode::from_parts(t.0, subcode)
    }
}

macro_rules! fb_status_codes {
    (
        $(
            $(#[$docs:meta])*
            ($code:expr, $subcode:expr, $konst:ident, $phrase:expr);
        )+
    ) => {
        impl FbStatusCode {
        $(
            $(#[$docs])*
            pub const $konst: FbStatusCode = FbStatusCode {
                code: unsafe { NonZeroU32::new_unchecked($code)},
                subcode: match $subcode {
                    0 => None,
                    v => Some(unsafe {NonZeroU16::new_unchecked(v)}),
                }
            };
        )+

        }

        fn canonical_reason(code: u32, subcode: Option<u16>) -> Option<&'static str> {
            match (code, subcode) {
                $(
                    ($code, None) if $subcode == 0 => Some($phrase),
                    ($code, Some($subcode)) if $subcode != 0 => Some($phrase),
                )+
                _ => None
            }
        }
    }
}

// https://developers.facebook.com/docs/graph-api/guides/error-handling#errorcodes
fb_status_codes! {
    /// OAuth / Access token invalid or expired
    (190, 0, INVALID_TOKEN, "Access token is invalid, expired, or revoked");
    /// Access token invalid (subcode)
    (190, 467, INVALID_TOKEN_SUBCODE, "Access token has expired, been revoked, or is otherwise invalid");
    /// API Session
    (102, 0, API_SESSION, "Login status or access token has expired, been revoked, or is otherwise invalid");
    /// API Unknown
    (1, 0, API_UNKNOWN, "Possibly a temporary issue due to downtime. Wait and retry the operation");
    /// API Service
    (2, 0, API_SERVICE, "Temporary issue due to downtime. Wait and retry the operation");
    /// API Method
    (3, 0, API_METHOD, "Capability or permissions issue. Ensure the app has the necessary permissions");
    /// API Too Many Calls
    (4, 0, API_TOO_MANY_CALLS, "Temporary issue due to throttling. Wait and retry, or examine request volume");
    /// API User Too Many Calls
    (17, 0, API_USER_TOO_MANY_CALLS, "Temporary issue due to throttling. Wait and retry, or examine request volume");
    /// API Permission Denied
    (10, 0, API_PERMISSION_DENIED, "Permission is either not granted or has been removed");
    /// API Permission (200-299)
    (200, 0, API_PERMISSION, "Permission is either not granted or has been removed");
    /// Application limit reached
    (341, 0, APPLICATION_LIMIT_REACHED, "Temporary issue due to downtime or throttling. Wait and retry");
    /// Temporarily blocked for policy violations
    (368, 0, TEMP_BLOCKED, "Temporarily blocked for policy violations. Wait and retry");
    /// Duplicate Post
    (506, 0, DUPLICATE_POST, "Duplicate posts cannot be published consecutively");
    /// Error Posting Link
    (1609005, 0, ERROR_POSTING_LINK, "Problem scraping data from the provided link. Check the URL");

    /// App Not Installed
    (190, 458, APP_NOT_INSTALLED, "The user has not logged into your app. Reauthenticate the user");
    /// User Checkpointed
    (190, 459, USER_CHECKPOINTED, "The user needs to log in at Facebook to correct an issue");
    /// Password Changed
    (190, 460, PASSWORD_CHANGED, "User must log in again or update password in OS settings");
    /// Expired token
    (190, 463, TOKEN_EXPIRED, "Login status or access token has expired, been revoked, or is invalid");
    /// Unconfirmed User
    (190, 464, UNCONFIRMED_USER, "User needs to log in at Facebook to correct an issue");
    /// Invalid Session
    (190, 492, INVALID_SESSION, "User associated with the Page access token does not have an appropriate role");
}

impl InvalidStatusCode {
    const fn new() -> InvalidStatusCode {
        Self { _priv: () }
    }
}

impl fmt::Debug for InvalidStatusCode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        f.debug_struct("InvalidStatusCode").finish()
    }
}

impl fmt::Display for InvalidStatusCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("invalid status code")
    }
}

impl Error for InvalidStatusCode {}
