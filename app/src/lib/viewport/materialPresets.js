/**
 * Curated engineering material library (`specs/studio_pbr_renderer.md` §1, §2.2).
 *
 * Each preset produces a standard `feature_engine::types::Material` record
 * (`{ name, density_kg_m3, appearance: { color, metalness, roughness } }`)
 * persisted in the `.waffle` file via `UiToEngine::EditMaterials`, plus
 * optional `finish` parameters (`clearcoat`, `clearcoatRoughness`,
 * `anisotropy`, `ior`, `opacity`) applied by `studioMaterial.js` when
 * constructing `THREE.MeshPhysicalMaterial` in Studio mode.
 */

export const MATERIAL_PRESETS = Object.freeze([
	// ── Metals ─────────────────────────────────────────────────────────────
	{
		id: 'aluminum-6061',
		category: 'Metals',
		name: 'Aluminum 6061',
		density_kg_m3: 2700,
		appearance: { color: [0.86, 0.88, 0.90], metalness: 0.92, roughness: 0.28 },
		finish: { anisotropy: 0.35, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.2 }
	},
	{
		id: 'stainless-304',
		category: 'Metals',
		name: 'Brushed Stainless 304',
		density_kg_m3: 8000,
		appearance: { color: [0.76, 0.77, 0.79], metalness: 0.95, roughness: 0.32 },
		finish: { anisotropy: 0.55, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.2 }
	},
	{
		id: 'polished-chrome',
		category: 'Metals',
		name: 'Polished Chrome',
		density_kg_m3: 7190,
		appearance: { color: [0.89, 0.91, 0.93], metalness: 1.0, roughness: 0.05 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.05 }
	},
	{
		id: 'steel-1018',
		category: 'Metals',
		name: 'Carbon Steel 1018',
		density_kg_m3: 7870,
		appearance: { color: [0.56, 0.58, 0.60], metalness: 0.88, roughness: 0.42 },
		finish: { anisotropy: 0.2, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.3 }
	},
	{
		id: 'cast-iron',
		category: 'Metals',
		name: 'Cast Iron',
		density_kg_m3: 7200,
		appearance: { color: [0.34, 0.35, 0.36], metalness: 0.78, roughness: 0.68 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.5 }
	},
	{
		id: 'titanium-grade5',
		category: 'Metals',
		name: 'Titanium Ti-6Al-4V',
		density_kg_m3: 4430,
		appearance: { color: [0.68, 0.66, 0.63], metalness: 0.90, roughness: 0.34 },
		finish: { anisotropy: 0.30, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.25 }
	},
	{
		id: 'brass-c360',
		category: 'Metals',
		name: 'Brass C360',
		density_kg_m3: 8500,
		appearance: { color: [0.91, 0.78, 0.45], metalness: 0.95, roughness: 0.22 },
		finish: { anisotropy: 0.25, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.15 }
	},
	{
		id: 'copper-c110',
		category: 'Metals',
		name: 'Copper C110',
		density_kg_m3: 8940,
		appearance: { color: [0.93, 0.62, 0.52], metalness: 0.96, roughness: 0.20 },
		finish: { anisotropy: 0.2, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.15 }
	},
	{
		id: 'gold-24k',
		category: 'Metals',
		name: 'Gold (24k)',
		density_kg_m3: 19300,
		appearance: { color: [0.98, 0.82, 0.35], metalness: 1.0, roughness: 0.12 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.1 }
	},

	// ── Anodized & Coated ──────────────────────────────────────────────────
	{
		id: 'anodized-red',
		category: 'Anodized',
		name: 'Anodized Red',
		density_kg_m3: 2700,
		appearance: { color: [0.82, 0.12, 0.15], metalness: 0.82, roughness: 0.26 },
		finish: { anisotropy: 0.18, anisotropyRotation: 0.0, clearcoat: 0.45, clearcoatRoughness: 0.18 }
	},
	{
		id: 'anodized-blue',
		category: 'Anodized',
		name: 'Anodized Blue',
		density_kg_m3: 2700,
		appearance: { color: [0.12, 0.40, 0.86], metalness: 0.82, roughness: 0.26 },
		finish: { anisotropy: 0.18, anisotropyRotation: 0.0, clearcoat: 0.45, clearcoatRoughness: 0.18 }
	},
	{
		id: 'anodized-black',
		category: 'Anodized',
		name: 'Anodized Black',
		density_kg_m3: 2700,
		appearance: { color: [0.13, 0.14, 0.16], metalness: 0.78, roughness: 0.30 },
		finish: { anisotropy: 0.15, anisotropyRotation: 0.0, clearcoat: 0.40, clearcoatRoughness: 0.22 }
	},

	// ── Polymers, Composites & Elastomers ──────────────────────────────────
	{
		id: 'abs-white',
		category: 'Polymers',
		name: 'ABS Matte White',
		density_kg_m3: 1040,
		appearance: { color: [0.91, 0.91, 0.89], metalness: 0.0, roughness: 0.48 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.12, clearcoatRoughness: 0.35 }
	},
	{
		id: 'abs-black',
		category: 'Polymers',
		name: 'ABS Matte Black',
		density_kg_m3: 1040,
		appearance: { color: [0.12, 0.12, 0.13], metalness: 0.0, roughness: 0.52 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.10, clearcoatRoughness: 0.40 }
	},
	{
		id: 'delrin-natural',
		category: 'Polymers',
		name: 'Delrin (POM) Natural',
		density_kg_m3: 1410,
		appearance: { color: [0.94, 0.93, 0.88], metalness: 0.0, roughness: 0.30 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.25, clearcoatRoughness: 0.22 }
	},
	{
		id: 'fr4-green',
		category: 'Composites',
		name: 'FR-4 Solder Mask Green',
		density_kg_m3: 1850,
		appearance: { color: [0.07, 0.36, 0.18], metalness: 0.0, roughness: 0.28 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.65, clearcoatRoughness: 0.15 }
	},
	{
		id: 'carbon-fiber',
		category: 'Composites',
		name: 'Carbon Fiber Composite',
		density_kg_m3: 1600,
		appearance: { color: [0.15, 0.16, 0.18], metalness: 0.25, roughness: 0.28 },
		finish: { anisotropy: 0.50, anisotropyRotation: 0.785, clearcoat: 0.85, clearcoatRoughness: 0.10 }
	},
	{
		id: 'polycarbonate-clear',
		category: 'Polymers',
		name: 'Polycarbonate Clear',
		density_kg_m3: 1200,
		appearance: { color: [0.88, 0.94, 0.98], metalness: 0.0, roughness: 0.08 },
		finish: {
			anisotropy: 0.0,
			anisotropyRotation: 0.0,
			clearcoat: 0.80,
			clearcoatRoughness: 0.05,
			ior: 1.58,
			opacity: 0.48
		}
	},
	{
		id: 'silicone-rubber',
		category: 'Elastomers',
		name: 'Silicone Rubber',
		density_kg_m3: 1150,
		appearance: { color: [0.22, 0.24, 0.27], metalness: 0.0, roughness: 0.86 },
		finish: { anisotropy: 0.0, anisotropyRotation: 0.0, clearcoat: 0.0, clearcoatRoughness: 0.8 }
	},

	// ── 3D Printed FDM Filaments (Shader Exterior Layer Lines) ─────────────
	{
		id: 'fdm-pla-orange',
		category: '3D Printed',
		name: '3D Printed PLA (Prusa Orange)',
		density_kg_m3: 1240,
		appearance: { color: [0.95, 0.36, 0.08], metalness: 0.0, roughness: 0.36 },
		finish: {
			anisotropy: 0.0,
			anisotropyRotation: 0.0,
			clearcoat: 0.28,
			clearcoatRoughness: 0.22,
			printLayerMm: 0.24
		}
	},
	{
		id: 'fdm-pla-white',
		category: '3D Printed',
		name: '3D Printed PLA (Matte White)',
		density_kg_m3: 1240,
		appearance: { color: [0.92, 0.91, 0.89], metalness: 0.0, roughness: 0.44 },
		finish: {
			anisotropy: 0.0,
			anisotropyRotation: 0.0,
			clearcoat: 0.16,
			clearcoatRoughness: 0.30,
			printLayerMm: 0.20
		}
	},
	{
		id: 'fdm-petg-black',
		category: '3D Printed',
		name: '3D Printed PETG (Gloss Black)',
		density_kg_m3: 1270,
		appearance: { color: [0.09, 0.10, 0.12], metalness: 0.0, roughness: 0.22 },
		finish: {
			anisotropy: 0.0,
			anisotropyRotation: 0.0,
			clearcoat: 0.58,
			clearcoatRoughness: 0.12,
			printLayerMm: 0.24
		}
	},
	{
		id: 'fdm-silk-pla-teal',
		category: '3D Printed',
		name: '3D Printed Silk PLA (Teal)',
		density_kg_m3: 1240,
		appearance: { color: [0.14, 0.68, 0.72], metalness: 0.55, roughness: 0.24 },
		finish: {
			anisotropy: 0.22,
			anisotropyRotation: 0.0,
			clearcoat: 0.40,
			clearcoatRoughness: 0.16,
			printLayerMm: 0.20
		}
	},
	{
		id: 'fdm-cf-nylon',
		category: '3D Printed',
		name: '3D Printed CF-Nylon (PA-CF)',
		density_kg_m3: 1180,
		appearance: { color: [0.16, 0.17, 0.18], metalness: 0.06, roughness: 0.58 },
		finish: {
			anisotropy: 0.0,
			anisotropyRotation: 0.0,
			clearcoat: 0.06,
			clearcoatRoughness: 0.48,
			printLayerMm: 0.16
		}
	}
]);

const BY_LOWER_NAME = new Map(MATERIAL_PRESETS.map((p) => [p.name.toLowerCase(), p]));

/**
 * Resolve optional Studio finish traits for a material by name (if it matches
 * a known engineering preset or a 3D-printed filament naming convention).
 * @param {string | null | undefined} materialName
 */
export function resolveStudioFinish(materialName) {
	if (typeof materialName === 'string' && materialName.length > 0) {
		const lower = materialName.toLowerCase();
		const hit = BY_LOWER_NAME.get(lower);
		if (hit?.finish) return hit.finish;
		if (lower.includes('3d print') || lower.includes('fdm') || lower.includes('pla') || lower.includes('petg')) {
			return {
				anisotropy: 0.0,
				anisotropyRotation: 0.0,
				clearcoat: 0.24,
				clearcoatRoughness: 0.24,
				printLayerMm: 0.24
			};
		}
	}
	return null;
}

/**
 * Convert a preset entry into an engine `Material` payload suitable for
 * `upsertMaterial` / `UiToEngine::EditMaterials`.
 * @param {typeof MATERIAL_PRESETS[number]} preset
 */
export function presetToEngineMaterial(preset) {
	return {
		name: preset.name,
		density_kg_m3: preset.density_kg_m3,
		appearance: {
			color: [...preset.appearance.color],
			metalness: preset.appearance.metalness,
			roughness: preset.appearance.roughness
		}
	};
}
