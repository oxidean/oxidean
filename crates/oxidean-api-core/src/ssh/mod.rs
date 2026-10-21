//! SSH-adjacent plumbing shared out of the transport listener: the host-key
//! directory resolution is consumed by `git::web_flow` for key material paths,
//! so it lives in core while the russh server stays in the `oxidean-api` shell.

pub mod host_keys;
