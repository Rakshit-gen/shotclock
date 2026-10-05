//! shotclock: find how far a camera's clock was off and write photo copies
//! with corrected dates.
pub mod build;
pub mod exif;
pub mod offset;
pub mod patch;
pub mod scan;
pub mod suggest;
pub mod write;
