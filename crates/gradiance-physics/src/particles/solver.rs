//! The particle-fluid step (brief S-10): pure maths, no ECS.
//!
//! Double-density relaxation (Clavet, Beaudoin and Poulin, "Particle-based
//! viscoelastic fluid simulation", SCA 2005), without the springs. Each
//! particle sees its neighbours within the radius `h` through a uniform
//! grid; a density and a near-density pressure push neighbours apart (or
//! pull them in, below the rest density), so the population keeps its
//! spacing and has a surface. Bodies are obstacles read as signed distance
//! fields (`geometry::sdf`): a particle inside one is pushed out and loses
//! its velocity into the surface, and the body takes the equal and opposite
//! impulse. Buoyancy, splashes and a pool's pressure all come from that one
//! exchange.
//!
//! Every buffer is kept between steps (`clear` keeps the capacity), so a
//! step allocates nothing once the population has stopped growing.

use bevy::math::Vec2;
use gradiance_geometry::sdf;
use gradiance_geometry::shape::ShapeDef;

/// The fluid's constants.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct FluidParams {
    /// The rest spacing between particles, in metres.
    pub spacing: f32,
    /// The interaction radius, in metres (a multiple of the spacing).
    pub radius: f32,
    /// The pressure stiffness (m/s²: displacement per step squared).
    pub stiffness: f32,
    /// The near-pressure stiffness (keeps particles from clumping).
    pub near_stiffness: f32,
    /// Linear viscosity between neighbours (1/s).
    pub viscosity: f32,
    /// The mass of one particle, in kg (areal: the world is 1 m deep).
    pub particle_mass: f32,
    /// How far a particle keeps from a body surface, in metres.
    pub contact_radius: f32,
    /// The share of the tangential speed a body contact removes per step.
    pub friction: f32,
}

impl FluidParams {
    /// Water at `spacing`: 1000 kg/m³ over a 1 m deep world is
    /// 1000 kg/m² of area, so a particle carries `1000 · spacing²` kg.
    pub fn water(spacing: f32) -> Self {
        Self {
            spacing,
            // Tuned by a scan (brief S-10 Outcome): three spacings of reach,
            // and enough viscosity that a pool settles in seconds.
            radius: 3.0 * spacing,
            stiffness: 1200.0 * spacing,
            near_stiffness: 1500.0 * spacing,
            viscosity: 40.0,
            particle_mass: 1000.0 * spacing * spacing,
            contact_radius: 0.5 * spacing,
            friction: 0.1,
        }
    }

    /// The kernel density of a particle in a square lattice at the rest
    /// spacing: the density each particle relaxes toward.
    pub fn rest_density(&self) -> f32 {
        let reach = (self.radius / self.spacing).ceil() as i32;
        let mut density = 0.0;
        for i in -reach..=reach {
            for j in -reach..=reach {
                if i == 0 && j == 0 {
                    continue;
                }
                let r = Vec2::new(i as f32, j as f32).length() * self.spacing;
                let q = r / self.radius;
                if q < 1.0 {
                    density += (1.0 - q) * (1.0 - q);
                }
            }
        }
        density
    }
}

/// The samples of the wall table across the interaction radius.
const WALL_SAMPLES: usize = 32;

/// What a flat solid wall at distance `d` adds to a particle's sums, as if
/// the solid were more fluid on the same lattice (the "boundary
/// deficiency" fix): without it a particle by a wall has half its
/// neighbours, reads a low density, and the wall feels too little
/// pressure, so a floating box sits too deep.
#[derive(Debug, Clone, PartialEq)]
pub struct WallTable {
    /// `(density, near density, push, near push)` at `d = radius · k / (N - 1)`.
    rows: [[f32; 4]; WALL_SAMPLES],
    radius: f32,
}

impl WallTable {
    /// Sums the solid half-plane's lattice points for each distance.
    pub fn new(params: &FluidParams) -> Self {
        let h = params.radius;
        let s = params.spacing;
        let reach = (h / s).ceil() as i32 + 1;
        let mut rows = [[0.0; 4]; WALL_SAMPLES];
        for (k, row) in rows.iter_mut().enumerate() {
            let d = h * k as f32 / (WALL_SAMPLES - 1) as f32;
            for i in 0..=reach {
                let x = d + 0.5 * s + i as f32 * s;
                for j in -reach..=reach {
                    let r = Vec2::new(x, j as f32 * s).length();
                    if r < h && r > 1e-6 {
                        let q = 1.0 - r / h;
                        let along = x / r;
                        row[0] += q * q;
                        row[1] += q * q * q;
                        row[2] += q * along;
                        row[3] += q * q * along;
                    }
                }
            }
        }
        Self { rows, radius: h }
    }

    /// The four sums at distance `d` (linear between samples; zero past the
    /// radius).
    fn at(&self, d: f32) -> [f32; 4] {
        if d >= self.radius {
            return [0.0; 4];
        }
        let t = (d.max(0.0) / self.radius) * (WALL_SAMPLES - 1) as f32;
        let k = (t as usize).min(WALL_SAMPLES - 2);
        let f = t - k as f32;
        let (a, b) = (self.rows[k], self.rows[k + 1]);
        [0, 1, 2, 3].map(|c| a[c] + (b[c] - a[c]) * f)
    }
}

/// One body the particles meet in a step.
#[derive(Debug, Clone, Copy)]
pub struct Obstacle<'a> {
    /// The body's shape, in its own frame.
    pub shape: &'a ShapeDef,
    /// The body's position (its frame's origin), in metres.
    pub pos: Vec2,
    /// The body's rotation, in radians.
    pub rot: f32,
    /// The velocity of the body's origin, in m/s.
    pub velocity: Vec2,
    /// The body's spin, in rad/s.
    pub spin: f32,
    /// The world box of the shape (a particle outside it, by more than the
    /// contact radius, is not tested).
    pub min: Vec2,
    /// See [`Obstacle::min`].
    pub max: Vec2,
}

impl Obstacle<'_> {
    /// The velocity of the body's material at the world point `p`.
    fn velocity_at(&self, p: Vec2) -> Vec2 {
        self.velocity + self.spin * (p - self.pos).perp()
    }
}

/// The impulse a step gave one obstacle: a linear impulse (N·s) and the
/// angular impulse about the obstacle's origin (N·m·s).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Exchange {
    /// The linear impulse.
    pub impulse: Vec2,
    /// The angular impulse about [`Obstacle::pos`].
    pub angular: f32,
}

/// The particle population and its working buffers.
#[derive(Debug, Default, Clone)]
pub struct Particles {
    /// Positions, in metres.
    pub pos: Vec<Vec2>,
    /// Velocities, in m/s.
    pub vel: Vec<Vec2>,
    prev: Vec<Vec2>,
    cell_of: Vec<u32>,
    order: Vec<u32>,
    cell_start: Vec<u32>,
    grid_min: Vec2,
    grid_cols: u32,
    grid_rows: u32,
    scratch: Vec<(usize, Vec2, f32)>,
    fill: Vec<u32>,
    /// Per particle: the nearest obstacle within the radius (index,
    /// distance from its surface, outward normal), or `u32::MAX`.
    wall: Vec<(u32, f32, Vec2)>,
}

impl Particles {
    /// The number of particles.
    pub fn len(&self) -> usize {
        self.pos.len()
    }

    /// Whether there are none.
    pub fn is_empty(&self) -> bool {
        self.pos.is_empty()
    }

    /// Adds a particle at rest.
    pub fn push(&mut self, pos: Vec2) {
        self.pos.push(pos);
        self.vel.push(Vec2::ZERO);
    }

    /// Fills the box `(min, max)` with particles on a square lattice at
    /// `spacing`, offset half a spacing from the edges.
    pub fn fill_box(&mut self, min: Vec2, max: Vec2, spacing: f32) {
        let mut y = min.y + 0.5 * spacing;
        while y < max.y {
            let mut x = min.x + 0.5 * spacing;
            while x < max.x {
                self.push(Vec2::new(x, y));
                x += spacing;
            }
            y += spacing;
        }
    }

    /// Removes every particle.
    pub fn clear(&mut self) {
        self.pos.clear();
        self.vel.clear();
    }

    /// Sorts the particles into a uniform grid of `cell`-sized cells
    /// (a counting sort: `order` lists particle indices cell by cell).
    fn build_grid(&mut self, cell: f32) {
        let (mut lo, mut hi) = (Vec2::splat(f32::MAX), Vec2::splat(f32::MIN));
        for p in &self.pos {
            lo = lo.min(*p);
            hi = hi.max(*p);
        }
        // A population that blew up would ask for a huge grid: cap it, and
        // let far particles share the edge cells (still correct, slower).
        let span = ((hi - lo) / cell).ceil().min(Vec2::splat(4096.0));
        self.grid_min = lo;
        self.grid_cols = span.x as u32 + 1;
        self.grid_rows = span.y as u32 + 1;
        let cells = (self.grid_cols * self.grid_rows) as usize;
        self.cell_start.clear();
        self.cell_start.resize(cells + 1, 0);
        self.cell_of.clear();
        for p in &self.pos {
            let c = self.cell_index(*p, cell);
            self.cell_of.push(c);
            self.cell_start[c as usize + 1] += 1;
        }
        for i in 0..cells {
            self.cell_start[i + 1] += self.cell_start[i];
        }
        self.order.clear();
        self.order.resize(self.pos.len(), 0);
        self.fill.clear();
        self.fill.extend_from_slice(&self.cell_start);
        for (i, c) in self.cell_of.iter().enumerate() {
            let slot = &mut self.fill[*c as usize];
            self.order[*slot as usize] = i as u32;
            *slot += 1;
        }
    }

    fn cell_coords(&self, p: Vec2, cell: f32) -> (u32, u32) {
        let c = ((p - self.grid_min) / cell).max(Vec2::ZERO);
        (
            (c.x as u32).min(self.grid_cols - 1),
            (c.y as u32).min(self.grid_rows - 1),
        )
    }

    fn cell_index(&self, p: Vec2, cell: f32) -> u32 {
        let (x, y) = self.cell_coords(p, cell);
        y * self.grid_cols + x
    }

    /// Advances the fluid by `dt` under `gravity` against `obstacles`, and
    /// adds each obstacle's impulse to `exchange` (same order; resized to
    /// match).
    pub fn step(
        &mut self,
        params: &FluidParams,
        rest_density: f32,
        walls: &WallTable,
        gravity: Vec2,
        dt: f32,
        obstacles: &[Obstacle<'_>],
        exchange: &mut Vec<Exchange>,
    ) {
        exchange.clear();
        exchange.resize(obstacles.len(), Exchange::default());
        let n = self.pos.len();
        if n == 0 || dt <= 0.0 {
            return;
        }
        let h = params.radius;
        self.build_grid(h);
        self.apply_viscosity(params, dt, h);
        self.prev.clear();
        self.prev.extend_from_slice(&self.pos);
        for i in 0..n {
            self.vel[i] += gravity * dt;
            self.pos[i] += self.vel[i] * dt;
        }
        self.find_walls(obstacles, h);
        self.relax(params, rest_density, walls, dt, h, obstacles, exchange);
        let inv_dt = 1.0 / dt;
        for i in 0..n {
            self.vel[i] = (self.pos[i] - self.prev[i]) * inv_dt;
        }
        self.collide(params, obstacles, exchange);
    }

    /// The neighbours of particle `i` within `h` (index, offset, distance),
    /// visited through `f`.
    fn for_neighbours(&self, i: usize, h: f32, mut f: impl FnMut(usize, Vec2, f32)) {
        let p = self.pos[i];
        let (cx, cy) = self.cell_coords(p, h);
        let x0 = cx.saturating_sub(1);
        let y0 = cy.saturating_sub(1);
        let x1 = (cx + 1).min(self.grid_cols - 1);
        let y1 = (cy + 1).min(self.grid_rows - 1);
        for y in y0..=y1 {
            for x in x0..=x1 {
                let c = (y * self.grid_cols + x) as usize;
                for k in self.cell_start[c]..self.cell_start[c + 1] {
                    let j = self.order[k as usize] as usize;
                    if j == i {
                        continue;
                    }
                    let d = self.pos[j] - p;
                    let r2 = d.length_squared();
                    if r2 < h * h && r2 > 1e-12 {
                        f(j, d, r2.sqrt());
                    }
                }
            }
        }
    }

    /// Linear viscosity between approaching neighbours (Clavet § 4.3).
    fn apply_viscosity(&mut self, params: &FluidParams, dt: f32, h: f32) {
        if params.viscosity <= 0.0 {
            return;
        }
        for i in 0..self.pos.len() {
            let mut dv = Vec2::ZERO;
            let vi = self.vel[i];
            self.for_neighbours(i, h, |j, d, r| {
                let n = d / r;
                let u = (vi - self.vel[j]).dot(n);
                if u > 0.0 {
                    let q = r / h;
                    dv -= 0.5 * dt * (1.0 - q) * params.viscosity * u * n;
                }
            });
            self.vel[i] += dv;
        }
    }

    /// Double-density relaxation (Clavet § 4.1), in place as the paper
    /// has it: each particle's pass moves its neighbours half the push and
    /// itself the other half, so later particles see earlier moves (a
    /// Gauss-Seidel sweep, which settles much faster than a Jacobi one).
    #[allow(clippy::too_many_arguments)]
    fn relax(
        &mut self,
        params: &FluidParams,
        rest_density: f32,
        walls: &WallTable,
        dt: f32,
        h: f32,
        obstacles: &[Obstacle<'_>],
        exchange: &mut [Exchange],
    ) {
        let n = self.pos.len();
        let mut near_list: Vec<(usize, Vec2, f32)> = std::mem::take(&mut self.scratch);
        for i in 0..n {
            near_list.clear();
            self.for_neighbours(i, h, |j, d, r| near_list.push((j, d, r)));
            let (o, d, normal) = self.wall[i];
            let wall = if o == u32::MAX { [0.0; 4] } else { walls.at(d) };
            let mut density = wall[0];
            let mut near = wall[1];
            for &(_, _, r) in &near_list {
                let q = 1.0 - r / h;
                density += q * q;
                near += q * q * q;
            }
            let pressure = params.stiffness * (density - rest_density);
            let near_pressure = params.near_stiffness * near;
            let mut dx = Vec2::ZERO;
            for &(j, d, r) in &near_list {
                let q = 1.0 - r / h;
                let push = dt * dt * (pressure * q + near_pressure * q * q) * (d / r);
                self.pos[j] += 0.5 * push;
                dx -= 0.5 * push;
            }
            if o != u32::MAX {
                // A wall neighbour pushes like a fluid one: half the pair's
                // displacement to the particle; the body takes the equal
                // and opposite impulse.
                let push = 0.5 * dt * dt * (pressure * wall[2] + near_pressure * wall[3]) * normal;
                dx += push;
                let impulse = params.particle_mass * push / dt;
                let at = self.pos[i] - normal * d;
                let e = &mut exchange[o as usize];
                e.impulse -= impulse;
                e.angular -= (at - obstacles[o as usize].pos).perp_dot(impulse);
            }
            self.pos[i] += dx;
        }
        self.scratch = near_list;
    }

    /// Finds each particle's nearest obstacle surface within `h`.
    fn find_walls(&mut self, obstacles: &[Obstacle<'_>], h: f32) {
        self.wall.clear();
        self.wall
            .resize(self.pos.len(), (u32::MAX, f32::MAX, Vec2::ZERO));
        for (o, obstacle) in obstacles.iter().enumerate() {
            let (lo, hi) = (obstacle.min - Vec2::splat(h), obstacle.max + Vec2::splat(h));
            let unturn = Vec2::from_angle(-obstacle.rot);
            let turn = Vec2::from_angle(obstacle.rot);
            for (i, p) in self.pos.iter().enumerate() {
                if p.x < lo.x || p.y < lo.y || p.x > hi.x || p.y > hi.y {
                    continue;
                }
                let local = unturn.rotate(*p - obstacle.pos);
                let d = sdf::eval(obstacle.shape, local);
                if d >= h || d >= self.wall[i].1 {
                    continue;
                }
                let g = sdf::gradient(obstacle.shape, local, 0.01 * h);
                if let Some(normal) = turn.rotate(g).try_normalize() {
                    self.wall[i] = (o as u32, d, normal);
                }
            }
        }
    }

    /// Pushes particles out of the obstacles, removes their velocity into
    /// the surface (and a share of the slip), and books the reaction.
    fn collide(
        &mut self,
        params: &FluidParams,
        obstacles: &[Obstacle<'_>],
        exchange: &mut [Exchange],
    ) {
        let r = params.contact_radius;
        for (o, obstacle) in obstacles.iter().enumerate() {
            let (lo, hi) = (obstacle.min - Vec2::splat(r), obstacle.max + Vec2::splat(r));
            let unturn = Vec2::from_angle(-obstacle.rot);
            let turn = Vec2::from_angle(obstacle.rot);
            for i in 0..self.pos.len() {
                let p = self.pos[i];
                if p.x < lo.x || p.y < lo.y || p.x > hi.x || p.y > hi.y {
                    continue;
                }
                let local = unturn.rotate(p - obstacle.pos);
                let d = sdf::eval(obstacle.shape, local);
                if d >= r {
                    continue;
                }
                let g = sdf::gradient(obstacle.shape, local, 0.25 * r);
                let Some(normal) = turn.rotate(g).try_normalize() else {
                    continue;
                };
                let surface = p + normal * (r - d);
                self.pos[i] = surface;
                let body_v = obstacle.velocity_at(surface);
                let rel = self.vel[i] - body_v;
                let vn = rel.dot(normal);
                let mut new_rel = rel;
                if vn < 0.0 {
                    new_rel -= vn * normal;
                    let tangent = new_rel;
                    new_rel = tangent * (1.0 - params.friction);
                }
                let new_v = body_v + new_rel;
                let impulse = params.particle_mass * (new_v - self.vel[i]);
                self.vel[i] = new_v;
                let e = &mut exchange[o];
                e.impulse -= impulse;
                e.angular -= (surface - obstacle.pos).perp_dot(impulse);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const G: Vec2 = Vec2::new(0.0, -10.0);

    fn walls() -> [ShapeDef; 3] {
        [
            ShapeDef::Box {
                width: 3.0,
                height: 0.2,
            },
            ShapeDef::Box {
                width: 0.2,
                height: 3.0,
            },
            ShapeDef::Box {
                width: 0.2,
                height: 3.0,
            },
        ]
    }

    fn tank(shapes: &[ShapeDef; 3]) -> Vec<Obstacle<'_>> {
        let poses = [
            Vec2::new(0.0, -0.1),
            Vec2::new(-1.1, 1.5),
            Vec2::new(1.1, 1.5),
        ];
        shapes
            .iter()
            .zip(poses)
            .map(|(shape, pos)| {
                let (min, max) = sdf::world_aabb(shape, pos, 0.0);
                Obstacle {
                    shape,
                    pos,
                    rot: 0.0,
                    velocity: Vec2::ZERO,
                    spin: 0.0,
                    min,
                    max,
                }
            })
            .collect()
    }

    /// A column released in a tank settles: it spreads to the walls, stops
    /// moving, keeps its area within 15 % (a shallow pool sits about 11 %
    /// loose: its boundary layers are a large share of it), and the floor
    /// carries its weight.
    #[test]
    fn a_column_settles_in_a_tank_and_the_floor_carries_its_weight() {
        let params = FluidParams::water(0.04);
        let rest = params.rest_density();
        let table = WallTable::new(&params);
        let mut fluid = Particles::default();
        fluid.fill_box(Vec2::new(-1.0, 0.0), Vec2::new(-0.2, 1.6), params.spacing);
        let n = fluid.len();
        let shapes = walls();
        let obstacles = tank(&shapes);
        let mut exchange = Vec::new();
        let dt = 1.0 / 240.0;
        for _ in 0..240 * 6 {
            fluid.step(&params, rest, &table, G, dt, &obstacles, &mut exchange);
        }
        let top = fluid.pos.iter().map(|p| p.y).fold(f32::MIN, f32::max);
        let speed = fluid.vel.iter().map(|v| v.length()).fold(0.0, f32::max);
        assert!(speed < 0.3, "settled: {speed} m/s");
        // 0.8 x 1.6 = 1.28 m² of water over a 2 m wide tank: 0.64 m deep.
        let area = 2.0 * top;
        assert!((area / 1.28 - 1.0).abs() < 0.15, "area {area} m²");
        // The floor's impulse over one step is the weight times dt.
        let weight = n as f32 * params.particle_mass * 10.0;
        let mut floor = 0.0;
        for _ in 0..60 {
            fluid.step(&params, rest, &table, G, dt, &obstacles, &mut exchange);
            floor += exchange[0].impulse.y;
        }
        let carried = -floor / (60.0 * dt);
        assert!(
            (carried / weight - 1.0).abs() < 0.15,
            "the floor carries {carried} N of {weight} N"
        );
    }
}
