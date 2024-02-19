//! The `polite` module defines a library-specific [`FauxPas`] alias for [`Error`], and an alias for
//! [`Result`], [`Polite`], using the [`Error`] alias.
use thiserror::Error;

/// The `Polite` type is an alias for `Result` using the library-defined [`FauxPas`].
pub type Polite<T> = Result<T, FauxPas>;

/// The `FauxPas` enum is a library-specific error conversion.
#[derive(Error, Debug)]
pub enum FauxPas {
    /// The `Auth` variant indicates an error occurred during the authorization process.
    #[error("Authorization failed.")]
    Auth,
    /// The `Env` variant represents error conversions from [`std::env::VarError`].
    #[error("Could not read environmental variables from .env.")]
    Env(#[from] std::env::VarError),
    /// The `FileName` variant indicates a malformed file name, from [`std::ffi::OsString`].
    #[error("Bad file name {0:?}.")]
    FileName(std::ffi::OsString),
    /// The `Int` variant represents error conversions from [`std::num::ParseIntError`],
    /// indicating a failure to parse an integer from a string.
    #[error("Could not parse integer from string.")]
    Int(#[from] std::num::ParseIntError),
    /// The `Io` variant represents error conversions from [`std::io::Error`].
    #[error("Input/output error from std.")]
    Io(#[from] std::io::Error),
    /// A `Parse` indicates an error occurred during parsing.
    #[error("Parse error.")]
    Parse,
    /// The `UserBuild` indicates an error occurred using a builder pattern.
    #[error("Value not provided for {value:?}.")]
    UserBuild {
        /// The `value` field returns messages on missing parameters in the builder function.
        value: Vec<String>,
    },
    /// The `BadIcon` results from a failed import of an icon image file into the Dioxus desktop
    /// app.
    #[cfg(feature = "icon")]
    #[cfg_attr(docsrs, doc(cfg(feature = "icon")))]
    #[error("Icon loading error.")]
    BadIcon(#[from] dioxus_desktop::tao::window::BadIcon),
    /// The `Bin` variant indicates a failure during binary encoding in crate `bincode`.
    #[cfg(feature = "bin")]
    #[cfg_attr(docsrs, doc(cfg(feature = "bin")))]
    #[error("Could not serialize to binary.")]
    Bin(#[from] Box<bincode::ErrorKind>),
    /// The `Byte` variant converts the error returned by the `byte_unit` crate.
    #[cfg(feature = "byte")]
    #[cfg_attr(docsrs, doc(cfg(feature = "byte")))]
    #[error("Byte conversion failed.")]
    Byte(#[from] byte_unit::ParseError),
    /// The `Csv` variant converts an error returned by the `csv` crate. Currently indicates
    /// failure to generate a csv writer.
    #[cfg(feature = "csvs")]
    #[cfg_attr(docsrs, doc(cfg(feature = "csvs")))]
    #[error("Could not make CSV writer.")]
    Csv(#[from] csv::Error),
    /// The `Http` variant converts an error from the `reqwest` crate.
    #[cfg(feature = "req")]
    #[cfg_attr(docsrs, doc(cfg(feature = "req")))]
    #[error("HTTP request error.")]
    Http(#[from] reqwest::Error),
    /// The `Image` variant converts an error from the `image` crate.
    #[cfg(feature = "img")]
    #[cfg_attr(docsrs, doc(cfg(feature = "img")))]
    #[error("Image processing error.")]
    Image(#[from] image::error::ImageError),
    /// The `Oauth2` variant converts an error from the `oauth2` crate.
    #[cfg(feature = "oauth")]
    #[cfg_attr(docsrs, doc(cfg(feature = "oauth")))]
    #[error("Oauth2 error: {0}")]
    Oauth2(
        #[from]
        oauth2::RequestTokenError<
            oauth2::reqwest::Error<reqwest::Error>,
            oauth2::StandardErrorResponse<oauth2::basic::BasicErrorResponseType>,
        >,
    ),
    /// The `Serialize` variant converts errors from the `serde` crate.
    #[cfg(feature = "serial")]
    #[cfg_attr(docsrs, doc(cfg(feature = "serial")))]
    #[error("Serialization error: {0}")]
    Serialize(#[from] serde::de::value::Error),
    /// The `SerdeJson` variant converts an error from the `serde_json` crate.
    #[cfg(feature = "serial")]
    #[cfg_attr(docsrs, doc(cfg(feature = "serial")))]
    #[error("Deserialization error: {0}")]
    SerdeJson(#[from] serde_json::Error),
    /// The `Sqlx` variant converts a general error from the `sqlx` crate.
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    #[error("Sqlx command error: {0}")]
    Sqlx(#[from] sqlx::Error),
    /// The `Migrate` variant converts a migration error from the `sqlx` crate.
    #[cfg(feature = "sql")]
    #[cfg_attr(docsrs, doc(cfg(feature = "sql")))]
    #[error("Sqlx migration error: {0}")]
    Migrate(#[from] sqlx::migrate::MigrateError),
    /// The `Uuid` variant converts an error from the `uuid` crate.
    #[cfg(feature = "uuids")]
    #[cfg_attr(docsrs, doc(cfg(feature = "uuids")))]
    #[error("Uuid conversion failed.")]
    Uuid(#[from] uuid::Error),
    /// The `Url` variant converts an error from the `url` crate.
    #[cfg(feature = "urls")]
    #[cfg_attr(docsrs, doc(cfg(feature = "urls")))]
    #[error("Parse error: {0}.")]
    Url(#[from] url::ParseError),
    /// The `BitMap` variant converts an error from the `plotters_bitmap` crate.
    #[cfg(feature = "plot")]
    #[cfg_attr(docsrs, doc(cfg(feature = "plot")))]
    #[error("Plotting backend error: {0}")]
    BitMap(#[from] plotters_bitmap::BitMapBackendError),
    /// The `Plot` variant converts an error from the `plotters` crate.
    #[cfg(feature = "plot")]
    #[cfg_attr(docsrs, doc(cfg(feature = "plot")))]
    #[error("Plotting drawing error: {0}")]
    Plot(#[from] plotters::drawing::DrawingAreaErrorKind<plotters_bitmap::BitMapBackendError>),
    /// The `TraceInit` variant converts an error from the `tracing_subscriber` crate.
    #[cfg(feature = "trace")]
    #[cfg_attr(docsrs, doc(cfg(feature = "trace")))]
    #[error("Problem initializing subscriber: {0}")]
    TraceInit(#[from] tracing_subscriber::util::TryInitError),
    /// The `BadTest` variant indicates a test failed, used by inner test functions.
    #[cfg(feature = "test")]
    #[cfg_attr(docsrs, doc(cfg(feature = "test")))]
    #[error("Test failed.")]
    BadTest,
    /// The `Improv` variant indicates a problem generating names or passwords using the underlying
    /// libraries `names` and `passwords`.
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    #[error("Generator option yielded none.")]
    Improv(String),
    /// The `Improv` variant indicates a problem generating names or passwords using the underlying
    /// libraries `names` and `passwords`.
    #[cfg(feature = "improv")]
    #[cfg_attr(docsrs, doc(cfg(feature = "improv")))]
    #[error("Password generation error: {0}")]
    Pass(String),
    /// The `Axum` variants converts an *axum::Error* from the `axum` crate.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[error("Axum error: {0}")]
    Axum(#[from] axum::Error),
    /// The `AxumHttp` variant converts an axum::http error from the `axum` crate.
    #[cfg(feature = "route")]
    #[cfg_attr(docsrs, doc(cfg(feature = "route")))]
    #[error("Axum http error: {0}")]
    AxumHttp(#[from] axum::http::Error),
    /// The `Infallible` variant converts from std::convert::Infallible.
    #[error("This operation is infallible.")]
    Infallible(#[from] std::convert::Infallible),

    /// The `Unknown` variant is a catch-all error variant for library operations.
    #[error("Unexpected error.")]
    Unknown,
}
