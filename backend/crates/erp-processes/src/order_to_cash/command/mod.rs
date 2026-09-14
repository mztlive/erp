//! Sales lifecycle commands and cross-domain approval transactions.
mod cancel;
mod create;
#[cfg(test)]
mod goods_service_cutover_tests;
mod identity;
#[cfg(test)]
mod related_scope_tests;
mod save;
mod submit;
mod void;
