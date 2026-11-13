//! UTC timestamps written in the format the TypeScript application uses
//! (`Date.prototype.toISOString`): millisecond precision and a `Z` suffix.

use time::OffsetDateTime;
use time::macros::format_description;

use crate::{Error, Result};

pub(crate) fn iso_now() -> Result<String> {
    OffsetDateTime::now_utc()
        .format(&format_description!(
            "[year]-[month]-[day]T[hour]:[minute]:[second].[subsecond digits:3]Z"
        ))
        .map_err(|error| Error::Message(error.to_string()))
}
