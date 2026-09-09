// Tonic-generated method signatures return `tonic::Status` directly in `Result`'s `Err`
// variant, dictated by the wire schema/tonic-build's own codegen shape - not something this
// codebase controls or can restructure without hand-editing generated code.
#[allow(clippy::enum_variant_names, clippy::result_large_err)]
pub mod v2 {
    tonic::include_proto!("synapse.v2");
}
