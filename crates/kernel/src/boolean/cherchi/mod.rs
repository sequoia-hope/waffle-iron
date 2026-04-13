//! Cherchi mesh arrangement — 1:1 Rust port of FastAndRobustMeshArrangements.
//!
//! MIT License — Copyright (c) 2020 Gianmarco Cherchi, Marco Livesu,
//! Riccardo Scateni e Marco Attene.
//!
//! Ported from: github.com/gcherchi/FastAndRobustMeshArrangements
//! Paper: Cherchi et al. 2020 "Fast and Robust Mesh Arrangements"
//! Paper: Cherchi et al. 2022 "Interactive and Robust Mesh Booleans"

pub(crate) mod common;
pub(crate) mod fast_trimesh;
pub(crate) mod tree;

pub(crate) mod aux_structure;
pub(crate) mod processing;
pub(crate) mod triangle_soup;
