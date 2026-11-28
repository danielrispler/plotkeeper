//! The extractors a map may name, one line each. Registering an extractor here also runs the contract cases on it.

use super::{
    Extractor, c::CIncludes, cargo::Cargo, dart::DartImports, dotnet::DotnetProjects,
    go::GoImports, java::JavaImports, php::PhpImports, python::PythonImports,
    rust_modules::RustModules, typescript::TypescriptImports,
};

pub(crate) const EXTRACTORS: &[&dyn Extractor] = &[
    &Cargo,
    &RustModules,
    &TypescriptImports,
    &GoImports,
    &JavaImports,
    &PythonImports,
    &DartImports,
    &PhpImports,
    &DotnetProjects,
    &CIncludes,
];
