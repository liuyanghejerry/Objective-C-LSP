//! ObjC-specific intelligence layer — the core differentiator.
//!
//! Handles Objective-C constructs that generic C/C++ LSPs mishandle:
//! - Multi-part selector completion ([obj method:arg1 withArg:arg2])
//! - @interface ↔ @implementation navigation
//! - Category aggregation
//! - @property rename coordination (getter + setter + ivar)
//! - Protocol conformance checking and stub generation

pub mod category;
pub mod header_nav;
pub mod property;
pub mod protocol;
pub mod selector;
