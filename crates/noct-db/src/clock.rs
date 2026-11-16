//! UTC timestamps written in the format the TypeScript application uses
//! (`Date.prototype.toISOString`): millisecond precision and a `Z` suffix.

use time::OffsetDateTime;
use time::macros::format_description;

use crate::{Error, Result};

pub(crate) fn iso_now() -> Result<String> {
    iso_at(OffsetDateTime::now_utc())
}

/// The instant in the same format. The instant must be UTC, because the suffix is literal.
pub(crate) fn iso_at(instant: OffsetDateTime) -> Result<String> {
    instant
        .format(&format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .map_err(|error| Error::Message(error.to_string()))
}
