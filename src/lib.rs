//! shotclock: find how far a camera's clock was off and write photo copies
//! with corrected dates.
pub mod exif;
pub mod build;
pub mod patch;
pub mod offset;
pub mod scan;
pub mod suggest;
pub mod write;
