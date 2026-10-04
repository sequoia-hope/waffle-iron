/**
 * Read-only agent tools (specs/waffle_mcp_server.md §2.5 Inspection). Definitions
 * only. `selection_get` is implemented in `../queries.js` (viewport state stays
 * with the host); the rest run in the engine
 * (`crates/wasm-bridge/src/tools/inspect.rs`, S3), routed by `../executor.js`.
 */
import { noArguments, uuid } from './common.js';
import { defsFor, engineRef } from './engineSchemas.js';

const readOnly = (title) => ({ title, readOnlyHint: true });

export const featureGetTool = {
	name: 'feature_get',
	description:
		'One feature of the open Part: its full Operation JSON (the same shape feature_edit takes), name, ' +
		'suppression, provenance and rebuild error. Lengths in meters, angles in degrees.',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Feature id from model_summary.') },
		required: ['feature_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			feature_id: { type: 'string' },
			name: { type: 'string' },
			suppressed: { type: 'boolean' },
			operation: { type: 'object' },
			provenance: { type: 'object' },
			error: { type: 'string' }
		},
		required: ['feature_id', 'name', 'suppressed', 'operation', 'provenance']
	},
	annotations: readOnly('Get feature')
};

export const selectionGetTool = {
	name: 'selection_get',
	description:
		"The user's current viewport selection: each picked face, edge or vertex as a GeomRef (a face ref is " +
		'usable as sketch_create plane), its kind and the body it belongs to; a selected datum plane has kind ' +
		'"DatumPlane" and a plane {origin, normal} to pass as sketch_create plane. Also the feature selected in ' +
		'the tree. In an open assembly, instance_path names the instance the user clicked; its picked face or ' +
		'edge is in the PART\'s space and is what connector_add takes as geom_ref. An empty selection is not an error.',
	inputSchema: noArguments,
	outputSchema: {
		type: 'object',
		properties: {
			selection: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						geom_ref: { type: 'object' },
						kind: { type: 'string', description: 'Face | Edge | Vertex …' },
						body_id: { type: ['string', 'null'] },
						signature: { type: 'object' }
					},
					required: ['geom_ref', 'kind', 'body_id']
				}
			},
			selected_feature_id: { type: ['string', 'null'] },
			instance_path: {
				type: ['array', 'null'],
				items: { type: 'string' },
				description: 'The clicked instance in an open assembly ([instance, member, …]); null otherwise.'
			}
		},
		required: ['selection', 'selected_feature_id', 'instance_path']
	},
	annotations: readOnly('Get selection')
};

const measuredValue = { type: 'number' };

export const bodyMeasureTool = {
	name: 'body_measure',
	description:
		'Volume (m³), surface area (m²), bounding box (m), topology counts and closedness of one body. ' +
		'method is "exact" when both quantities were integrated from the B-Rep, else "mesh" (render-mesh ' +
		'values, low on curved faces); methods and exact_unavailable say which quantity fell back and why. ' +
		'The bounding box always comes from the render mesh.',
	inputSchema: {
		type: 'object',
		properties: { body_id: { type: 'string', description: 'Body id from model_summary.bodies.' } },
		required: ['body_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string' },
			volume_m3: measuredValue,
			surface_area_m2: measuredValue,
			method: { type: 'string', enum: ['exact', 'mesh'] },
			methods: {
				type: 'object',
				properties: { volume: { type: 'string' }, surface_area: { type: 'string' } }
			},
			exact_unavailable: { type: 'object' },
			bbox_min: { type: 'array', items: { type: 'number' } },
			bbox_max: { type: 'array', items: { type: 'number' } },
			face_count: { type: 'integer' },
			edge_count: { type: 'integer' },
			vertex_count: { type: 'integer' },
			closed: { type: 'boolean' }
		},
		required: [
			'body_id',
			'volume_m3',
			'surface_area_m2',
			'method',
			'bbox_min',
			'bbox_max',
			'face_count',
			'edge_count',
			'vertex_count',
			'closed'
		]
	},
	annotations: readOnly('Measure body')
};

const measureOperand = {
	type: 'object',
	description:
		'A measurement operand: {"type":"body","body_id":…} (from model_summary.bodies), ' +
		'{"type":"entity","geom_ref":…} (a face/edge/vertex GeomRef from face_list), ' +
		'{"type":"name","name":…} (an entity or body name from entity_name / names_list), or ' +
		'{"type":"point","point":[x,y,z]} (meters).',
	properties: {
		type: { type: 'string', enum: ['body', 'entity', 'name', 'point'] },
		body_id: { type: 'string' },
		geom_ref: { type: 'object' },
		name: { type: 'string' },
		point: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 }
	},
	required: ['type']
};

export const measureDistanceTool = {
	name: 'measure_distance',
	description:
		'Minimum distance in meters between two operands (bodies, faces, edges, vertices or points), ' +
		'with the closest point on each and what that point lies on. method is "exact" when the kernel ' +
		'certified the number from the analytic surfaces, else "mesh" and chord_bound_m is the band the ' +
		'true value lies within. along asks for the gap along a direction instead of the minimum ' +
		'distance, and comes back negative when the operands overlap along it. An axis operand and a ' +
		'mesh-backed imported body are not supported yet: they refuse rather than approximate.',
	inputSchema: {
		type: 'object',
		properties: {
			a: measureOperand,
			b: measureOperand,
			along: {
				type: ['array', 'null'],
				items: { type: 'number' },
				minItems: 3,
				maxItems: 3,
				description: 'Measure the gap along this direction instead of the minimum distance.'
			}
		},
		required: ['a', 'b'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			distance_m: { type: 'number' },
			method: { type: 'string', enum: ['exact', 'mesh'] },
			chord_bound_m: { type: 'number' },
			points: {
				type: 'array',
				items: { type: 'array', items: { type: 'number' } },
				minItems: 2,
				maxItems: 2
			},
			on: {
				type: 'array',
				items: {
					type: ['object', 'null'],
					properties: { kind: { type: 'object' }, kernel_id: { type: 'integer' } }
				}
			},
			along: { type: ['array', 'null'], items: { type: 'number' } }
		},
		required: ['distance_m', 'method', 'points', 'on']
	},
	annotations: readOnly('Measure distance')
};

export const measureInterferenceTool = {
	name: 'measure_interference',
	description:
		'Whether two bodies share interior volume, touch, or are apart — computed by running the ' +
		"kernel's own Intersect boolean on copies of them, so the answer is the same geometry a " +
		'Subtract would act on. kind is "interferes" (with volume_m3 and, per lump of the overlap, ' +
		'its volume, centroid and bounding box so you know where to look), "contact" (they touch but ' +
		'share no interior; evidence says how that was established), or "disjoint" (with the same ' +
		'closest-point answer measure_distance gives). If the kernel cannot run the boolean for this ' +
		'pair it is an ERROR naming the wall, never "disjoint": "could not tell" is not "no collision".',
	inputSchema: {
		type: 'object',
		properties: {
			a: { type: 'string', description: 'Body id from model_summary.bodies.' },
			b: { type: 'string', description: 'Body id from model_summary.bodies.' }
		},
		required: ['a', 'b'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			a: { type: 'string' },
			b: { type: 'string' },
			kind: { type: 'string', enum: ['interferes', 'contact', 'disjoint'] },
			volume_m3: { type: 'number' },
			method: { type: 'string', enum: ['exact', 'mesh'] },
			chord_bound_m: { type: 'number' },
			regions: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						volume_m3: { type: 'number' },
						centroid: { type: 'array', items: { type: 'number' } },
						aabb_min: { type: 'array', items: { type: 'number' } },
						aabb_max: { type: 'array', items: { type: 'number' } }
					}
				}
			},
			evidence: {
				type: 'string',
				enum: ['empty_intersection_at_zero_distance', 'sliver_intersection']
			},
			sliver_volume_m3: { type: 'number' },
			closest: { type: 'object' },
			distance: { type: 'object' }
		},
		required: ['a', 'b', 'kind']
	},
	annotations: readOnly('Measure interference')
};

export const measureMassTool = {
	name: 'measure_mass',
	description:
		'Volume, surface area, centroid and the inertia tensor about the centroid of one body, in SI ' +
		'(m³, m², meters, kg·m²). method is "exact" when every face was integrated in closed form ' +
		'(planar faces, cylinder and cone bands, and the circular caps and bore walls a boolean ' +
		'leaves behind), else "mesh" and chord_bound_m is the tessellation band — a mesh volume is ' +
		'LOW by the chord deficit, never high. density_kg_m3 defaults to 1 because the document ' +
		'carries no material table, so mass_kg is then numerically the volume; pass the density to ' +
		'scale mass and inertia. The answer always says which density it used.',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'Body id from model_summary.bodies.' },
			density_kg_m3: {
				type: ['number', 'null'],
				description: 'Material density in kg/m³. Defaults to 1.'
			}
		},
		required: ['body_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string' },
			volume_m3: { type: 'number' },
			surface_area_m2: { type: 'number' },
			centroid: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
			inertia_at_centroid: {
				type: 'array',
				items: { type: 'array', items: { type: 'number' } },
				minItems: 3,
				maxItems: 3
			},
			principal_moments: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
			principal_axes: {
				type: 'array',
				items: { type: 'array', items: { type: 'number' } },
				minItems: 3,
				maxItems: 3
			},
			density_kg_m3: { type: 'number' },
			mass_kg: { type: 'number' },
			method: { type: 'string', enum: ['exact', 'mesh'] },
			chord_bound_m: { type: 'number' }
		},
		required: [
			'body_id',
			'volume_m3',
			'surface_area_m2',
			'centroid',
			'inertia_at_centroid',
			'principal_moments',
			'principal_axes',
			'density_kg_m3',
			'mass_kg',
			'method'
		]
	},
	annotations: readOnly('Measure mass')
};

export const measureSectionTool = {
	name: 'measure_section',
	description:
		'Cut bodies with a plane and get the cap as DATA: per body, the cap boundary loops as 2D ' +
		'analytic curves in the cut plane\'s own frame (line, circle, ellipse — never flattened to ' +
		'chords unless the kernel could not keep the curve analytic, which it says per loop), each ' +
		"loop's signed area (positive outer, negative hole), the net area and the area centroid. The " +
		'cut is the kernel\'s own Intersect against a half-space, keeping the side the normal points ' +
		'AWAY from, so a section and a Subtract against the same plane agree. basis is the frame the ' +
		'loops live in (world = origin + u·u_axis + v·v_axis) — use it rather than deriving your own, ' +
		'or the loops will be rotated against it. plane is {origin, normal}, a planar face GeomRef ' +
		'from face_list, {"plane":"XY"|"XZ"|"YZ"} for a datum, or {"name":"Plate.top"} for a named ' +
		'planar face. body_ids defaults to every body. A plane that MISSES a body is an empty loops ' +
		'list with kept_material saying which side it missed on — not an error; a body the kernel ' +
		'REFUSED to cut is named in declines instead, never as an empty section. cap_shared_with_model ' +
		'means at least one cap face came from the coplanar-overlay path rather than from the cut\'s ' +
		'own lineage — the signature of a plane coplanar with a face of this body; it describes how ' +
		'the KEPT cap was attributed, so it can be true on one side of such a cut and false on the ' +
		'other. Lengths in meters, areas in m².',
	inputSchema: {
		type: 'object',
		properties: {
			body_ids: {
				type: ['array', 'null'],
				items: { type: 'string' },
				description:
					'Body ids (or display names) from model_summary.bodies. Omit for every body of the open Part.'
			},
			plane: {
				type: 'object',
				description:
					'{origin:[x,y,z], normal:[x,y,z]} | a planar face GeomRef | {"plane":"XY"} | {"name":"Plate.top"}.'
			}
		},
		required: ['plane'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			plane: {
				type: 'object',
				properties: {
					origin: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
					normal: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 }
				}
			},
			basis: {
				type: 'object',
				properties: {
					origin: { type: 'array', items: { type: 'number' } },
					u_axis: { type: 'array', items: { type: 'number' } },
					v_axis: { type: 'array', items: { type: 'number' } },
					w_axis: { type: 'array', items: { type: 'number' } }
				}
			},
			total_area_m2: { type: 'number' },
			bodies: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						body_id: { type: 'string' },
						area_m2: { type: 'number' },
						centroid_uv: { type: ['array', 'null'], items: { type: 'number' } },
						centroid: { type: ['array', 'null'], items: { type: 'number' } },
						centroid_exact: { type: 'boolean' },
						method: { type: 'string', enum: ['exact', 'mesh'] },
						cap_shared_with_model: { type: 'boolean' },
						kept_material: { type: 'boolean' },
						loops: {
							type: 'array',
							items: {
								type: 'object',
								properties: {
									signed_area_m2: { type: 'number' },
									exact: { type: 'boolean' },
									kind: { type: 'string', enum: ['outer', 'hole'] },
									curves: { type: 'array', items: { type: 'object' } }
								},
								required: ['signed_area_m2', 'exact', 'kind', 'curves']
							}
						}
					},
					required: [
						'body_id',
						'loops',
						'area_m2',
						'centroid_exact',
						'method',
						'cap_shared_with_model',
						'kept_material'
					]
				}
			},
			declines: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						body_id: { type: 'string' },
						kind: { type: 'string', enum: ['not_supported', 'failed'] },
						reason: { type: 'string' }
					}
				}
			},
			name_warnings: { type: 'array', items: { type: 'string' } }
		},
		required: ['plane', 'bodies', 'total_area_m2']
	},
	annotations: readOnly('Measure section')
};

export const measureThicknessTool = {
	name: 'measure_thickness',
	description:
		'The wall thickness of a body, SAMPLED: points are laid out on every face and a ray is cast ' +
		'inward from each one to the first face opposite. **For a wall, read min_wall_m, not min_m.** ' +
		'min_m is the shortest cast ANYWHERE on the body, and every ACUTE edge is a sliver of material ' +
		'— a 4 mm slot through a 10/7 mm tube reports min_m 0.043 mm where its wall is 3 mm, and a ' +
		'taper approaches zero at its sharp corner. min_wall_m is the shortest cast between two faces ' +
		'that do NOT meet at an edge, so it leaves every corner reading out: 3.000 mm on that same ' +
		'tube, and equal to min_m on a plate, which has no corner to leave out. Each site says which ' +
		'it is with faces_share_an_edge. min_wall_m is absent only when every site crossed a corner. ' +
		'Also reports mean_m, max_m, a histogram of the sites, and the thinnest site of each kind — ' +
		'where it is and the two faces it spans, each with its persistent id (DECIMAL STRINGS: ids ' +
		'above 2^53 are not exact as JSON numbers) and its entity_name if it has one. method is always ' +
		'"sampled": both minima are UPPER BOUNDS, because a wall thinner than spacing_m between two ' +
		'sites is never looked at. Pass spacing_m under the width of the web you care about to be sure ' +
		'it was sampled; the answer always reports the spacing_m it used and how many samples it took. ' +
		'Each individual cast is refined onto the analytic surfaces (a plate reports its thickness, and ' +
		'a tube r_outer − r_inner, to rounding), and refined says how many were; declines counts the ' +
		'sites that produced nothing, so an answer covering little of the body says so. Neither number ' +
		'is the body\'s medial axis. Lengths in meters.',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'Body id (or name) from model_summary.bodies.' },
			spacing_m: {
				type: ['number', 'null'],
				description:
					'Largest gap between neighbouring sample sites on one face, in meters. Omit for the ' +
					"default (the body's bounding diagonal / 32)."
			}
		},
		required: ['body_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string' },
			min_m: { type: 'number' },
			min_wall_m: { type: ['number', 'null'] },
			mean_m: { type: 'number' },
			max_m: { type: 'number' },
			thinnest: {
				type: 'object',
				properties: {
					thickness_m: { type: 'number' },
					point: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
					opposite: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
					from: { type: 'object' },
					to: { type: 'object' },
					faces_share_an_edge: { type: 'boolean' }
				},
				required: ['thickness_m', 'point', 'opposite', 'from', 'to', 'faces_share_an_edge']
			},
			thinnest_wall: {
				type: ['object', 'null'],
				properties: {
					thickness_m: { type: 'number' },
					point: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
					opposite: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
					from: { type: 'object' },
					to: { type: 'object' },
					faces_share_an_edge: { type: 'boolean' }
				}
			},
			histogram: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						lo_m: { type: 'number' },
						hi_m: { type: 'number' },
						count: { type: 'integer' }
					}
				}
			},
			samples: { type: 'integer' },
			spacing_m: { type: 'number' },
			chord_bound_m: { type: 'number' },
			refined: { type: 'integer' },
			declines: {
				type: 'object',
				properties: {
					no_hit: { type: 'integer' },
					below_self_band: { type: 'integer' },
					no_surface: { type: 'integer' }
				}
			},
			method: { type: 'string', enum: ['sampled'] }
		},
		required: [
			'body_id',
			'min_m',
			'mean_m',
			'max_m',
			'thinnest',
			'histogram',
			'samples',
			'spacing_m',
			'method'
		]
	},
	annotations: readOnly('Measure thickness')
};

export const faceListTool = {
	name: 'face_list',
	description:
		'Every face of a body as the GeomRef the viewport hands out when the user picks it (usable as a ' +
		'sketch_create plane), with its topological signature, in a deterministic order. filter narrows ' +
		'the list with TopoQuery filters (tie_break is ignored). A face that has been given a name ' +
		'(entity_name) also carries it as name.',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'Body id from model_summary.bodies.' },
			filter: engineRef('TopoQuery')
		},
		required: ['body_id'],
		additionalProperties: false,
		$defs: defsFor('TopoQuery')
	},
	outputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string' },
			faces: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						geom_ref: { type: 'object' },
						signature: { type: 'object' },
						name: { type: 'string', description: 'The entity name pointing at this face, if any.' }
					},
					required: ['geom_ref', 'signature']
				}
			}
		},
		required: ['body_id', 'faces']
	},
	annotations: readOnly('List faces')
};

export const entityListTool = {
	name: 'entity_list',
	description:
		'Every face, edge or vertex of one body, with its geometric content (Q6). Each entity carries its ' +
		'persistent id (pid/root_pid — content-derived, so it survives rebuilds and booleans; DECIMAL STRINGS, because an id above 2^53 is not exact as a JSON number — treat them as opaque, never as arithmetic), the GeomRef ' +
		'that names it, its entity_name if it has one, its signature (surface type, area, centroid, normal, ' +
		'bbox, and the axis descriptor of a cylinder/cone/sphere/torus), and its axis LINE (origin + ' +
		'direction) where it has one. An EDGE also carries length.arc_length_m — the length ALONG the curve, ' +
		'with curve_type and method: exact for lines, circles and circular arcs; quadrature (plus residual_m) ' +
		'for ellipse and hyperbola arcs, which have no closed form; chords (a lower bound) for SSI curves and ' +
		'imported polylines. A VERTEX carries position. body carries the whole body principal axes and ' +
		'centroid (the same integration measure_mass reports), or says why it has none. Order is by ' +
		'persistent id, so two listings of the same geometry are comparable. filter arms compose: query ' +
		'(TopoQuery filters), name (a * / ? glob over the entity name), bbox ([min, max] in meters, keeping ' +
		'entities whose own bbox is inside it). excluded_unevaluable counts entities a filter arm dropped ' +
		'because their own data could not answer it, so an empty list does not read as "nothing matched" ' +
		'when it means "nothing could be asked"; unresolved_names lists the body entity names that now ' +
		'bind to nothing, and name_warnings on an entity means its name was rebound by geometry after its ' +
		'persistent id went away and may be naming the wrong entity. A spherical axis has a centre and no ' +
		'direction. Lengths in meters.',
	inputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string', description: 'Body id or body name from model_summary.bodies.' },
			kind: { type: 'string', enum: ['face', 'edge', 'vertex'] },
			filter: {
				type: 'object',
				properties: {
					query: engineRef('TopoQuery'),
					name: { type: 'string', description: 'Glob over the entity name: * and ?.' },
					bbox: {
						type: 'array',
						items: { type: 'array', items: { type: 'number' }, minItems: 3, maxItems: 3 },
						minItems: 2,
						maxItems: 2,
						description: '[min, max] corners in meters; keeps entities contained in the box.'
					}
				},
				additionalProperties: false
			}
		},
		required: ['body_id', 'kind'],
		additionalProperties: false,
		$defs: defsFor('TopoQuery')
	},
	outputSchema: {
		type: 'object',
		properties: {
			body_id: { type: 'string' },
			kind: { type: 'string', enum: ['face', 'edge', 'vertex'] },
			count: { type: 'number' },
			entities: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						// Decimal STRINGS, not numbers: a persistent id is a
						// content-seeded u64 and a JSON number here is an f64,
						// so an id above 2^53 would reach a caller as a
						// different entity (`waffle_types::pid_str`). Pinned
						// by `file-format/tests/schema_golden.rs::
						// no_pid_field_crosses_as_a_number`, which scans the
						// generated relay manifest and so sees this file.
						pid: { type: 'string', pattern: '^[0-9]+$' },
						root_pid: { type: 'string', pattern: '^[0-9]+$' },
						geom_ref: { type: 'object' },
						name: { type: 'string' },
						name_warnings: { type: 'array', items: { type: 'string' } },
						signature: { type: 'object' },
						axis: {
							type: 'object',
							properties: {
								kind: { type: 'string' },
								origin: { type: 'array', items: { type: 'number' } },
								direction: { type: 'array', items: { type: 'number' } },
								radius: { type: 'number' }
							},
							required: ['kind', 'origin']
						},
						length: {
							type: 'object',
							properties: {
								arc_length_m: { type: 'number' },
								curve_type: { type: 'string' },
								closed: { type: 'boolean' },
								method: { type: 'string', enum: ['exact', 'quadrature', 'chords'] },
								residual_m: { type: 'number' },
								chord_bound_m: { type: 'number' }
							},
							required: ['arc_length_m', 'curve_type', 'closed', 'method']
						},
						length_unavailable: { type: 'string' },
						position: { type: 'array', items: { type: 'number' } }
					},
					required: ['signature']
				}
			},
			body: {
				type: 'object',
				properties: {
					centroid: { type: 'array', items: { type: 'number' } },
					principal_moments: { type: 'array', items: { type: 'number' } },
					principal_axes: { type: 'array', items: { type: 'array', items: { type: 'number' } } },
					method: { type: 'string', enum: ['exact', 'mesh'] },
					unavailable: { type: 'string' }
				}
			},
			excluded_unevaluable: { type: 'number' },
			unresolved_names: { type: 'array', items: { type: 'string' } }
		},
		required: ['body_id', 'kind', 'count', 'entities', 'body', 'excluded_unevaluable']
	},
	annotations: readOnly('List entities')
};

export const sketchRegionsTool = {
	name: 'sketch_regions',
	description:
		'The closed regions of a completed sketch. A region equal to one whole loop carries ' +
		'profile_entity_ids — pass that list as ExtrudeParams/RevolveParams.profile_entity_ids to extrude ' +
		'exactly that loop. Sub-regions of overlapping shapes have profile_entity_ids null. area_m2 in m².',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Id of a Sketch feature.') },
		required: ['feature_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			feature_id: { type: 'string' },
			regions: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						profile_entity_ids: { type: ['array', 'null'], items: { type: 'integer' } },
						area_m2: { type: 'number' }
					},
					required: ['profile_entity_ids', 'area_m2']
				}
			}
		},
		required: ['feature_id', 'regions']
	},
	annotations: readOnly('Sketch regions')
};

export const sketch3dGetTool = {
	name: 'sketch3d_get',
	description:
		'What a 3D sketch EVALUATED to: where every point landed (a point attached to model geometry ' +
		'resolves only at rebuild, so feature_get shows the declaration, not the answer), and the chains ' +
		'its segments form. A chain is a run of lines and arcs joined end to end; the graph splits into ' +
		'one chain per run at every point where the number of segments is not two, so a truss centre-line ' +
		'graph comes back as one chain per member. tangent_joints[i] says whether the joint between edge i ' +
		'and edge i+1 is smooth (a fillet) rather than a corner — that is what decides a mitre from a bend ' +
		'when a sweep runs along it. warnings lists any attached point that resolved with a caveat — a ' +
		'BestEffort pick that re-bound onto the NEAREST entity after the geometry moved names its point ' +
		'and the distance. A suppressed, rolled-back or failed sketch answers NotEvaluated.',
	inputSchema: {
		type: 'object',
		properties: { feature_id: uuid('Id of a Sketch3d feature.') },
		required: ['feature_id'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			feature_id: { type: 'string' },
			entity_count: { type: 'integer' },
			points: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						id: { type: 'integer' },
						xyz: { type: 'array', items: { type: 'number' } }
					},
					required: ['id', 'xyz']
				}
			},
			chains: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						closed: { type: 'boolean' },
						length_m: { type: 'number' },
						tangent_joints: { type: 'array', items: { type: 'boolean' } },
						edges: { type: 'array', items: { type: 'object' } }
					},
					required: ['closed', 'length_m', 'tangent_joints', 'edges']
				}
			},
			warnings: {
				type: 'array',
				items: { type: 'string' },
				description:
					'One entry per attached point that resolved with a caveat, prefixed "point <id>:".'
			}
		},
		required: ['feature_id', 'entity_count', 'points', 'chains', 'warnings']
	},
	annotations: readOnly('3D sketch geometry')
};

export const expressionEvaluateTool = {
	name: 'expression_evaluate',
	description:
		'Evaluate an expression against the design parameters AND the live model, as a dimension field would. ' +
		'mm-space: a bare number means millimeters for lengths (degrees for angles); unit suffixes (mm, cm, m, ' +
		'in, ft, deg, rad) and parameter names are allowed. Measurement functions read the model by ENTITY NAME ' +
		'(entity_name / body_rename): distance(a, b), angle(a, b), length(edge), radius(entity), area(face), ' +
		'volume(body) — mm, degrees, mm^2 and mm^3 respectively, so sqrt(area(top)) is a length a depth takes ' +
		'and area(top) is not. mass(body) is reserved and refuses until a material table exists. Returns ' +
		'value_mm plus the dimension the expression produced, or value_mm null with the evaluation error. Pass ' +
		'dimension to have it judged as that kind of field would judge it: "25deg" asked for as a Length is an ' +
		'error, not 25 mm.',
	inputSchema: {
		type: 'object',
		properties: {
			expression: {
				type: 'string',
				description: 'e.g. "width / 2", "1.5in", or "distance(wall_a, wall_b) / 2".'
			},
			dimension: {
				type: 'string',
				enum: ['Length', 'Angle', 'Count', 'Ratio'],
				description:
					'Optional: the kind of field this expression is meant for. A committed unit that does not fit is ' +
					'refused instead of read as a plain number.'
			}
		},
		required: ['expression'],
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			expression: { type: 'string' },
			value_mm: { type: ['number', 'null'] },
			dimension: {
				type: 'string',
				description:
					'What the expression IS: "length", "angle", "ratio", a composite such as "length^2", or ' +
					'"unitless" when no suffix committed a dimension (a plain number any field accepts).'
			},
			error: { type: 'string' }
		},
		required: ['expression', 'value_mm']
	},
	annotations: readOnly('Evaluate expression')
};

export const parametersGetTool = {
	name: 'parameters_get',
	description:
		'Read the design-parameter table. Each parameter carries its expression, the value and dimension the ' +
		'last rebuild evaluated, which PARAMETERS it reads (depends_on), which ENTITIES it measures ' +
		'(measures), which parameters read it (used_by) and which ' +
		'FEATURE FIELDS read it (used_by_fields) — so you can see what a change will move before making it. ' +
		'An expression that does not evaluate reports its own error with value_mm null; the rest of the table ' +
		'still answers. Dependency cycles are listed in cycles, each as the names around the loop. ' +
		'scope chooses which table: tab (the default) answers the open Part tab\'s rows PLUS every ' +
		'DOCUMENT row it inherits, each marked with its own scope and shadowed:true on a document row the ' +
		'tab redeclares — one answer to what an expression here can read. document answers the ' +
		'document-wide table alone (the only scope an Assembly or Drawing tab has). instance, with ' +
		'instance_id, answers a part\'s table as that placed instance builds it: an overridden row carries ' +
		'override and its value_mm is the pinned magnitude.',
	inputSchema: {
		type: 'object',
		properties: {
			scope: {
				type: 'string',
				enum: ['tab', 'document', 'instance'],
				description: 'Which table to read. Default tab.'
			},
			instance_id: uuid('Required for scope instance: the assembly instance to read.')
		},
		additionalProperties: false
	},
	outputSchema: {
		type: 'object',
		properties: {
			parameters: {
				type: 'array',
				items: {
					type: 'object',
					properties: {
						id: { type: 'string' },
						name: { type: 'string' },
						expression: { type: 'string' },
						value_mm: {
							type: ['number', 'null'],
							description:
								'The working-space magnitude: millimeters for a length, degrees for an angle, the plain ' +
								'number otherwise. Null when the expression does not evaluate.'
						},
						dimension: {
							type: 'object',
							description:
								'What the expression PRODUCED, not what was declared. `committed` false means no unit ' +
								'suffix committed a dimension, so the value adopts whatever field reads it. `kind` is ' +
								'absent for a composite such as length^2, which no field accepts.',
							properties: {
								length: { type: 'integer' },
								angle: { type: 'integer' },
								committed: { type: 'boolean' },
								label: { type: 'string' },
								kind: { type: 'string', enum: ['Length', 'Angle', 'Count', 'Ratio'] }
							}
						},
						unit: {
							type: 'string',
							enum: ['Length', 'Angle', 'Count', 'Ratio'],
							description: 'The DECLARED dimension, when the author declared one.'
						},
						comment: { type: 'string' },
						error: { type: 'string' },
						scope: {
							type: 'string',
							enum: ['tab', 'document', 'instance'],
							description:
								'Where this row lives. instance marks a row this instance OVERRIDES; tab a row of ' +
								'the part\'s own table; document a row inherited from the document-wide table.'
						},
						shadowed: {
							type: 'boolean',
							description:
								'A document row the open tab redeclares, so the tab\'s row wins. Listed anyway: ' +
								'this is the answer to "why is my document variable not driving this".'
						},
						override: {
							type: 'number',
							description:
								'scope instance only: the working-space magnitude this instance pins the parameter ' +
								'to. The row\'s own expression is not evaluated.'
						},
						depends_on: {
							type: 'array',
							items: { type: 'string' },
							description:
								'PARAMETER names this expression reads directly. Empty if it does not parse, and ' +
								'empty for an expression that only measures the model — see `measures`.'
						},
						measures: {
							type: 'array',
							items: { type: 'string' },
							description:
								'Entity names this expression MEASURES (D2), e.g. the `plate` of ' +
								'`volume(plate) / 1000`. A separate namespace from depends_on: this parameter ' +
								'depends on that geometry being built, so its value moves when the feature that ' +
								'owns the entity changes. Absent when the expression measures nothing.'
						},
						used_by: {
							type: 'array',
							items: { type: 'string' },
							description:
								'Parameters whose expressions read this one, WITHIN the same table. A document row ' +
								'and a tab row of the same name are two parameters, and the tab one shadows the ' +
								'document one rather than depending on it.'
						},
						used_by_fields: {
							type: 'array',
							description: 'Feature fields driven by an expression that reads this parameter.',
							items: {
								type: 'object',
								properties: {
									feature_id: { type: 'string' },
									feature: { type: 'string' },
									field: {
										type: 'string',
										description: 'e.g. "depth", "angle", "spacing", "dimension #3", "arg teeth".'
									},
									expression: { type: 'string' }
								}
							}
						}
					},
					required: ['id', 'name', 'expression', 'value_mm', 'depends_on', 'used_by', 'used_by_fields']
				}
			},
			cycles: {
				type: 'array',
				description:
					'Each dependency cycle as the names around the loop with the first repeated at the end, ' +
					'e.g. ["a","b","a"]. Empty when the table is acyclic.',
				items: { type: 'array', items: { type: 'string' } }
			},
			scope: {
				type: 'string',
				enum: ['tab', 'document', 'instance'],
				description: 'The scope that was read.'
			},
			instance_id: { type: 'string', description: 'scope instance only.' },
			overrides_matching_no_parameter: {
				type: 'array',
				items: { type: 'string' },
				description:
					'scope instance only: override names the part declares no parameter for. Each is also a ' +
					'loud rebuild error; named here so you can find them without reading the error list.'
			},
			used_by_fields_scope: {
				type: 'string',
				description:
					'scope document only: says that field readers are NOT reported, because a document ' +
					"parameter's readers span every tab and this call holds only the open one's tree."
			}
		},
		required: ['parameters', 'cycles']
	},
	annotations: readOnly('Read parameters')
};
