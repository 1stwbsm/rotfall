"""Procedural Blender scene for ROTFALL's menu art. No external assets.

    blender -b -P tools/menu_scene.py -- scene  OUT.png WIDTH HEIGHT SAMPLES
    blender -b -P tools/menu_scene.py -- logo   OUT.png WIDTH HEIGHT SAMPLES FONT.ttf
    blender -b -P tools/menu_scene.py -- thumb  OUT.png WIDTH HEIGHT SAMPLES FONT.ttf
    blender -b -P tools/menu_scene.py -- icon   OUT.png WIDTH HEIGHT SAMPLES FONT.ttf
"""
import math
import random
import sys

import bmesh
import bpy
from mathutils import Euler, Matrix, Vector

argv = sys.argv[sys.argv.index("--") + 1:]
MODE, OUT, W, H, SAMPLES = argv[0], argv[1], int(argv[2]), int(argv[3]), int(argv[4])
FONT = argv[5] if len(argv) > 5 else None
R = random.Random(1337)

bpy.ops.wm.read_factory_settings(use_empty=True)
scene = bpy.context.scene
scene.render.engine = "CYCLES"
scene.cycles.device = "CPU"
scene.cycles.samples = SAMPLES
scene.cycles.use_denoising = True
scene.cycles.max_bounces = 5
scene.cycles.volume_bounces = 1
scene.cycles.seed = 7
scene.render.resolution_x = W
scene.render.resolution_y = H
scene.render.filepath = OUT
scene.render.image_settings.file_format = "PNG"
scene.view_settings.view_transform = "AgX"
scene.view_settings.look = "AgX - High Contrast"
scene.view_settings.exposure = -0.9


def material(name, color=(0.5, 0.5, 0.5), rough=0.7, metal=0.0, emit=None, strength=0.0):
    m = bpy.data.materials.new(name)
    m.use_nodes = True
    b = m.node_tree.nodes["Principled BSDF"]
    b.inputs["Base Color"].default_value = (*color, 1)
    b.inputs["Roughness"].default_value = rough
    b.inputs["Metallic"].default_value = metal
    if emit:
        b.inputs["Emission Color"].default_value = (*emit, 1)
        b.inputs["Emission Strength"].default_value = strength
    return m


def noisy(name, a, b, scale=3.0, rough=(0.45, 0.9), metal=0.0, bump=0.25):
    """Two-colour noise material with matching roughness and bump."""
    m = material(name, metal=metal)
    t = m.node_tree
    p = t.nodes["Principled BSDF"]
    co = t.nodes.new("ShaderNodeTexCoord")
    n = t.nodes.new("ShaderNodeTexNoise")
    n.inputs["Scale"].default_value = scale
    n.inputs["Detail"].default_value = 9
    n.inputs["Roughness"].default_value = 0.6
    t.links.new(co.outputs["Object"], n.inputs["Vector"])
    ramp = t.nodes.new("ShaderNodeValToRGB")
    ramp.color_ramp.elements[0].position = 0.35
    ramp.color_ramp.elements[0].color = (*a, 1)
    ramp.color_ramp.elements[1].position = 0.68
    ramp.color_ramp.elements[1].color = (*b, 1)
    t.links.new(n.outputs[0], ramp.inputs[0])
    t.links.new(ramp.outputs[0], p.inputs["Base Color"])
    rr = t.nodes.new("ShaderNodeMapRange")
    rr.inputs["To Min"].default_value = rough[0]
    rr.inputs["To Max"].default_value = rough[1]
    t.links.new(n.outputs[0], rr.inputs["Value"])
    t.links.new(rr.outputs[0], p.inputs["Roughness"])
    bp = t.nodes.new("ShaderNodeBump")
    bp.inputs["Strength"].default_value = bump
    t.links.new(n.outputs[0], bp.inputs["Height"])
    t.links.new(bp.outputs[0], p.inputs["Normal"])
    return m


def from_bm(name, bm, mat, smooth=False):
    me = bpy.data.meshes.new(name)
    bm.to_mesh(me)
    bm.free()
    me.materials.append(mat)
    if smooth:
        for poly in me.polygons:
            poly.use_smooth = True
    ob = bpy.data.objects.new(name, me)
    scene.collection.objects.link(ob)
    return ob


def box(bm, c, s, rot=(0, 0, 0)):
    m = Matrix.Translation(c) @ Euler(rot).to_matrix().to_4x4() @ Matrix.Diagonal((*s, 1))
    bmesh.ops.create_cube(bm, size=1.0, matrix=m)


def cyl(bm, a, b, r1, r2, seg=12):
    a, b = Vector(a), Vector(b)
    d = b - a
    q = Vector((0, 0, 1)).rotation_difference(d.normalized()).to_matrix().to_4x4()
    m = Matrix.Translation((a + b) / 2) @ q
    bmesh.ops.create_cone(bm, cap_ends=True, segments=seg, radius1=r1, radius2=r2, depth=d.length, matrix=m)


def light(kind, loc, energy, color, size=1.0, rot=None):
    d = bpy.data.lights.new("l", kind)
    d.energy = energy
    d.color = color
    if kind in ("POINT", "SPOT"):
        d.shadow_soft_size = size
    elif kind == "AREA":
        d.size = size
    ob = bpy.data.objects.new("l", d)
    ob.location = loc
    if rot:
        ob.rotation_euler = rot
    scene.collection.objects.link(ob)
    return ob


def aim(ob, target):
    ob.rotation_euler = (Vector(target) - ob.location).to_track_quat("-Z", "Y").to_euler()


# ------------------------------------------------------------------ logo
def rust_text(text, voxel=0.0045, dent=0.024):
    """Extruded lettering in pitted, rusted steel. Returns the mesh object, centered on the origin."""
    txt = bpy.data.curves.new("t", "FONT")
    txt.body = text
    if FONT:
        txt.font = bpy.data.fonts.load(FONT)
    txt.extrude = 0.11
    txt.bevel_depth = 0.022
    txt.bevel_resolution = 3
    txt.align_x = "CENTER"
    txt.align_y = "CENTER"
    txt.space_character = 1.05
    ob = bpy.data.objects.new("t", txt)
    scene.collection.objects.link(ob)
    bpy.context.view_layer.objects.active = ob
    ob.select_set(True)
    bpy.ops.object.convert(target="MESH")
    ob = bpy.context.view_layer.objects.active
    # Rust and blood eat into plate steel; the noise also dents the geometry.
    m = noisy("rust", (0.09, 0.03, 0.012), (0.56, 0.47, 0.36), scale=5.0, rough=(0.3, 0.85), metal=0.85, bump=0.8)
    ob.data.materials.append(m)
    bpy.ops.object.mode_set(mode="EDIT")
    bpy.ops.mesh.select_all(action="SELECT")
    bpy.ops.mesh.quads_convert_to_tris()
    bpy.ops.object.mode_set(mode="OBJECT")
    rem = ob.modifiers.new("r", "REMESH")
    rem.mode = "VOXEL"
    rem.voxel_size = voxel
    tex = bpy.data.textures.new("n", "CLOUDS")
    tex.noise_scale = 0.09
    tex.noise_depth = 4
    dis = ob.modifiers.new("d", "DISPLACE")
    dis.texture = tex
    dis.strength = dent
    for poly in ob.data.polygons:
        poly.use_smooth = True
    return ob


if MODE == "logo":
    scene.render.film_transparent = True
    scene.view_settings.look = "AgX - High Contrast"
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.03, 0.04, 0.05, 1)
    scene.world = world
    ob = rust_text("ROTFALL")
    ob.rotation_euler = (math.radians(90), 0, 0)
    cam = bpy.data.cameras.new("c")
    cam.lens = 70
    camo = bpy.data.objects.new("c", cam)
    scene.collection.objects.link(camo)
    scene.camera = camo
    # Fit the word to the frame with room for bloom.
    width = ob.dimensions.x
    camo.location = (0.0, -width * 2.05, 0.12)
    aim(camo, (0, 0, 0.03))
    aim(light("AREA", (-3, -3, 3), 260, (1.0, 0.72, 0.42), size=3), (0, 0, 0))
    aim(light("AREA", (3.5, -2.2, -1.2), 60, (0.2, 0.85, 1.0), size=3), (0, 0, 0))
    aim(light("AREA", (0, 2.5, 2.5), 420, (1.0, 0.14, 0.05), size=4), (0, 0, 0))
    bpy.ops.render.render(write_still=True)
    sys.exit(0)

# ------------------------------------------------------------------ app icon
if MODE == "icon":
    # One bold letter in front of the reactor's glow: the only thing that survives 16 pixels.
    scene.render.film_transparent = True
    scene.view_settings.look = "AgX - Very High Contrast"
    scene.view_settings.exposure = 0.0
    world = bpy.data.worlds.new("w")
    world.use_nodes = True
    world.node_tree.nodes["Background"].inputs[0].default_value = (0.02, 0.03, 0.04, 1)
    scene.world = world
    ob = rust_text("R", voxel=0.006, dent=0.012)
    ob.rotation_euler = (math.radians(90), 0, 0)
    h = ob.dimensions.y
    k = 1.0 / h
    ob.scale = (k, k, k)
    # Dark steel tile with softened corners, and a glowing disc behind the letter.
    bm = bmesh.new()
    box(bm, (0, 0.22, 0), (1.5, 0.12, 1.5))
    tile = from_bm("tile", bm, material("tile", (0.012, 0.02, 0.028), 0.45, 0.7))
    tile.modifiers.new("b", "BEVEL").width = 0.26
    tile.modifiers["b"].segments = 8
    bm = bmesh.new()
    cyl(bm, (0, 0.15, 0), (0, 0.14, 0), 0.62, 0.62, 48)
    from_bm("glow", bm, material("glow", (0, 0, 0), 0.5, emit=(0.02, 0.42, 1.0), strength=0.62))
    cam = bpy.data.cameras.new("c")
    cam.type = "ORTHO"
    cam.ortho_scale = 1.56
    camo = bpy.data.objects.new("c", cam)
    scene.collection.objects.link(camo)
    scene.camera = camo
    camo.location = (0, -6, 0)
    aim(camo, (0, 0, 0))
    aim(light("AREA", (-2, -3, 2.5), 90, (1.0, 0.78, 0.5), size=2.5), (0, 0, 0))
    aim(light("AREA", (2, -2, -1.5), 26, (1.0, 0.25, 0.08), size=2), (0, 0, 0))
    bpy.ops.render.render(write_still=True)
    sys.exit(0)

# ------------------------------------------------------------------ materials
asphalt = noisy("asphalt", (0.012, 0.012, 0.014), (0.05, 0.05, 0.052), scale=18, rough=(0.1, 0.62), bump=0.45)
concrete = noisy("concrete", (0.05, 0.045, 0.04), (0.2, 0.17, 0.14), scale=2.2, rough=(0.65, 0.95), bump=0.7)
brick = noisy("brick", (0.07, 0.025, 0.018), (0.19, 0.08, 0.05), scale=3.0, rough=(0.7, 0.95), bump=0.8)
rust = noisy("rustcar", (0.05, 0.02, 0.01), (0.21, 0.09, 0.05), scale=6, rough=(0.35, 0.9), metal=0.5, bump=0.6)
steel = noisy("steel", (0.04, 0.045, 0.05), (0.16, 0.17, 0.18), scale=9, rough=(0.3, 0.7), metal=0.9, bump=0.2)
hazard = material("hazard", (0.6, 0.38, 0.03), 0.6)
black = material("black", (0.012, 0.012, 0.014), 0.5)
flesh = noisy("flesh", (0.035, 0.05, 0.03), (0.12, 0.11, 0.08), scale=7, rough=(0.5, 0.85), bump=0.5)
cloth = noisy("cloth", (0.015, 0.015, 0.02), (0.06, 0.045, 0.04), scale=14, rough=(0.8, 1.0), bump=0.5)
eye = material("eye", (0, 0, 0), 0.5, emit=(1.0, 0.13, 0.03), strength=70)
core_glow = material("core", (0, 0, 0), 0.4, emit=(0.06, 0.62, 1.0), strength=14)
beam_glow = material("beam", (0, 0, 0), 0.4, emit=(0.08, 0.66, 1.0), strength=20)
window_warm = material("win", (0, 0, 0), 0.5, emit=(1.0, 0.45, 0.13), strength=7)
ember = material("ember", (0, 0, 0), 0.5, emit=(1.0, 0.22, 0.03), strength=30)
fire = material("fire", (0, 0, 0), 0.5, emit=(1.0, 0.27, 0.04), strength=30)
lampmat = material("lamp", (0, 0, 0), 0.5, emit=(1.0, 0.72, 0.4), strength=30)
tree_mat = material("tree", (0.012, 0.01, 0.009), 0.9)

# ------------------------------------------------------------------ world, sky, fog
world = bpy.data.worlds.new("w")
world.use_nodes = True
scene.world = world
t = world.node_tree
bg = t.nodes["Background"]
co = t.nodes.new("ShaderNodeTexCoord")
sep = t.nodes.new("ShaderNodeSeparateXYZ")
t.links.new(co.outputs["Generated"], sep.inputs[0])
n = t.nodes.new("ShaderNodeTexNoise")
n.inputs["Scale"].default_value = 3.2
n.inputs["Detail"].default_value = 10
t.links.new(co.outputs["Generated"], n.inputs["Vector"])
ramp = t.nodes.new("ShaderNodeValToRGB")
ramp.color_ramp.elements[0].position = 0.0
ramp.color_ramp.elements[0].color = (0.24, 0.075, 0.03, 1)
ramp.color_ramp.elements[1].position = 0.32
ramp.color_ramp.elements[1].color = (0.006, 0.012, 0.024, 1)
math_add = t.nodes.new("ShaderNodeMath")
math_add.operation = "ADD"
mul = t.nodes.new("ShaderNodeMath")
mul.operation = "MULTIPLY"
mul.inputs[1].default_value = 0.14
t.links.new(n.outputs[0], mul.inputs[0])
t.links.new(sep.outputs["Z"], math_add.inputs[0])
t.links.new(mul.outputs[0], math_add.inputs[1])
t.links.new(math_add.outputs[0], ramp.inputs[0])
t.links.new(ramp.outputs[0], bg.inputs[0])
bg.inputs[1].default_value = 0.34

fog_bm = bmesh.new()
box(fog_bm, (0, 75, 18), (220, 240, 48))
fog_mat = bpy.data.materials.new("fog")
fog_mat.use_nodes = True
nt = fog_mat.node_tree
nt.nodes.clear()
out = nt.nodes.new("ShaderNodeOutputMaterial")
vs = nt.nodes.new("ShaderNodeVolumeScatter")
vs.inputs["Color"].default_value = (0.82, 0.86, 0.95, 1)
vs.inputs["Anisotropy"].default_value = 0.25
vco = nt.nodes.new("ShaderNodeTexCoord")
vn = nt.nodes.new("ShaderNodeTexNoise")
vn.inputs["Scale"].default_value = 0.08
vn.inputs["Detail"].default_value = 3
nt.links.new(vco.outputs["Object"], vn.inputs["Vector"])
# Ground-hugging: thick at street level, clearing with height, broken up by noise.
vsep = nt.nodes.new("ShaderNodeSeparateXYZ")
nt.links.new(vco.outputs["Object"], vsep.inputs[0])
mr = nt.nodes.new("ShaderNodeMapRange")
mr.inputs["From Min"].default_value = -18
mr.inputs["From Max"].default_value = 4
mr.inputs["To Min"].default_value = 0.016
mr.inputs["To Max"].default_value = 0.0022
nt.links.new(vsep.outputs["Z"], mr.inputs["Value"])
vm = nt.nodes.new("ShaderNodeMath")
vm.operation = "MULTIPLY"
nt.links.new(mr.outputs[0], vm.inputs[0])
vr = nt.nodes.new("ShaderNodeMapRange")
vr.inputs["From Min"].default_value = 0.3
vr.inputs["From Max"].default_value = 0.7
vr.inputs["To Min"].default_value = 0.35
vr.inputs["To Max"].default_value = 1.5
nt.links.new(vn.outputs[0], vr.inputs["Value"])
nt.links.new(vr.outputs[0], vm.inputs[1])
nt.links.new(vm.outputs[0], vs.inputs["Density"])
nt.links.new(vs.outputs[0], out.inputs["Volume"])
from_bm("fog", fog_bm, fog_mat)

# ------------------------------------------------------------------ ground
g = bmesh.new()
box(g, (0, 80, -0.25), (260, 320, 0.5))
from_bm("ground", g, asphalt)
paint = material("paint", (0.3, 0.24, 0.1), 0.75)
g = bmesh.new()
for y in range(6, 170, 8):
    if R.random() > 0.2:
        box(g, (R.uniform(-0.04, 0.04), y, 0.004), (0.17, 3.6 * R.uniform(0.6, 1), 0.01), (0, 0, R.uniform(-0.01, 0.01)))
from_bm("lines", g, paint)
# Standing water: glossy puddles that catch the reactor.
water = material("water", (0.004, 0.005, 0.007), 0.04)
g = bmesh.new()
for _ in range(34):
    c = (R.uniform(-9, 9), R.uniform(9, 70), 0.006)
    m = Matrix.Translation(c) @ Matrix.Diagonal((R.uniform(0.4, 1.8), R.uniform(0.6, 3.2), 1, 1))
    bmesh.ops.create_circle(g, cap_ends=True, segments=24, radius=1, matrix=m)
from_bm("puddles", g, water)

# ------------------------------------------------------------------ ruined blocks
def block(x0, x1, y0, y1, floors, mat, side):
    """A facade of stacked wall panels with holes and a jagged, collapsed top."""
    bm = bmesh.new()
    win = bmesh.new()
    fh = 3.4
    nx = max(2, int((y1 - y0) / 3.0))
    face = x0  # The side that looks at the street.
    profile = [floors * R.uniform(0.55, 1.0) for _ in range(nx)]
    for i in range(1, nx):
        profile[i] = profile[i] * 0.6 + profile[i - 1] * 0.4
    for i in range(nx):
        y = y0 + (i + 0.5) * (y1 - y0) / nx
        w = (y1 - y0) / nx
        top = profile[i]
        for f in range(int(top)):
            z = (f + 0.5) * fh
            # Pier and spandrel leave a window opening in every bay.
            box(bm, (face, y - w * 0.36, z), (0.5, w * 0.28, fh))
            box(bm, (face, y + w * 0.36, z), (0.5, w * 0.28, fh))
            box(bm, (face, y, z - fh * 0.34), (0.5, w * 0.44, fh * 0.32))
            box(bm, (face, y, z + fh * 0.4), (0.5, w * 0.44, fh * 0.2))
            if R.random() < 0.13:
                box(win, (face + side * 1.2, y, z), (0.1, w * 0.42, fh * 0.45))
            if f % 1 == 0:
                box(bm, ((x0 + x1) / 2, y, f * fh + 0.1), (abs(x1 - x0), w, 0.25))
        # Back wall stops the sky showing straight through.
        box(bm, (x1, y, top * fh * 0.5), (0.5, w, top * fh))
        # Rubble and rebar at the broken top.
        for _ in range(4):
            box(bm, (face + R.uniform(-0.4, 0.4), y + R.uniform(-w, w) * 0.5, top * fh + R.uniform(0, 1.2)),
                (R.uniform(0.3, 0.8), R.uniform(0.5, 1.3), R.uniform(0.3, 1.5)),
                (R.uniform(-0.5, 0.5), R.uniform(-0.5, 0.5), R.uniform(-0.5, 0.5)))
        for _ in range(3):
            base = Vector((face, y + R.uniform(-w, w) * 0.5, top * fh))
            cyl(bm, base, base + Vector((R.uniform(-0.4, 0.4), R.uniform(-0.4, 0.4), R.uniform(0.7, 1.9))), 0.03, 0.02, 5)
    from_bm("block", bm, mat)
    if win.verts:
        from_bm("windows", win, window_warm)
    else:
        win.free()


y = 8.0
while y < 150:
    span = R.uniform(14, 26)
    block(-11 - R.uniform(0, 2.5), -30, y, y + span, R.randint(3, 7), R.choice([concrete, brick]), -1)
    y += span + R.uniform(0.5, 7)
y = 3.0
while y < 150:
    span = R.uniform(14, 26)
    block(11 + R.uniform(0, 2.5), 30, y, y + span, R.randint(3, 7), R.choice([concrete, brick]), 1)
    y += span + R.uniform(0.5, 7)

# Rubble spilling onto the road from both kerbs.
g = bmesh.new()
for _ in range(320):
    side = R.choice((-1, 1))
    x = side * (R.uniform(0, 1) ** 1.6 * 5.5 + 6.5)
    s = R.uniform(0.1, 0.7)
    box(g, (x, R.uniform(0, 130), s * 0.3), (s * R.uniform(0.7, 1.8), s, s * R.uniform(0.4, 1)),
        (R.uniform(-0.5, 0.5), R.uniform(-0.5, 0.5), R.uniform(0, 6)))
from_bm("rubble", g, concrete)


# ------------------------------------------------------------------ wrecks, lamps, fire
def make_flame_volume():
    """Emission-only volume: turbulent noise, eaten away with height and toward the box edge."""
    m = bpy.data.materials.new("flame")
    m.use_nodes = True
    nt = m.node_tree
    nt.nodes.clear()
    out = nt.nodes.new("ShaderNodeOutputMaterial")
    em = nt.nodes.new("ShaderNodeEmission")
    co = nt.nodes.new("ShaderNodeTexCoord")
    mp = nt.nodes.new("ShaderNodeMapping")
    mp.inputs["Scale"].default_value = (1.0, 1.0, 0.45)
    nt.links.new(co.outputs["Object"], mp.inputs["Vector"])
    nz = nt.nodes.new("ShaderNodeTexNoise")
    nz.inputs["Scale"].default_value = 2.4
    nz.inputs["Detail"].default_value = 8
    nz.inputs["Distortion"].default_value = 1.0
    nt.links.new(mp.outputs[0], nz.inputs["Vector"])
    sp = nt.nodes.new("ShaderNodeSeparateXYZ")
    nt.links.new(co.outputs["Object"], sp.inputs[0])
    # Radial and height falloff, in the object space of each 3x5x3.6 box centered on its fire.
    rad = nt.nodes.new("ShaderNodeVectorMath")
    rad.operation = "LENGTH"
    sc = nt.nodes.new("ShaderNodeVectorMath")
    sc.operation = "MULTIPLY"
    sc.inputs[1].default_value = (0.75, 0.42, 0.45)
    nt.links.new(co.outputs["Object"], sc.inputs[0])
    nt.links.new(sc.outputs[0], rad.inputs[0])
    sub = nt.nodes.new("ShaderNodeMath")
    sub.operation = "SUBTRACT"
    nt.links.new(nz.outputs[0], sub.inputs[0])
    mul = nt.nodes.new("ShaderNodeMath")
    mul.operation = "MULTIPLY"
    mul.inputs[1].default_value = 0.46
    nt.links.new(rad.outputs["Value"], mul.inputs[0])
    nt.links.new(mul.outputs[0], sub.inputs[1])
    rng = nt.nodes.new("ShaderNodeMapRange")
    rng.inputs["From Min"].default_value = 0.14
    rng.inputs["From Max"].default_value = 0.42
    rng.inputs["To Min"].default_value = 0.0
    rng.inputs["To Max"].default_value = 1.0
    nt.links.new(sub.outputs[0], rng.inputs["Value"])
    col = nt.nodes.new("ShaderNodeValToRGB")
    col.color_ramp.elements[0].position = 0.0
    col.color_ramp.elements[0].color = (0.5, 0.03, 0.0, 1)
    col.color_ramp.elements[1].position = 1.0
    col.color_ramp.elements[1].color = (1.0, 0.42, 0.07, 1)
    nt.links.new(rng.outputs[0], col.inputs[0])
    nt.links.new(col.outputs[0], em.inputs["Color"])
    st = nt.nodes.new("ShaderNodeMath")
    st.operation = "MULTIPLY"
    st.inputs[1].default_value = 7.0
    nt.links.new(rng.outputs[0], st.inputs[0])
    nt.links.new(st.outputs[0], em.inputs["Strength"])
    nt.links.new(em.outputs[0], out.inputs["Volume"])
    return m


flame_volume = make_flame_volume()


def car(x, y, yaw, burning=False):
    bm = bmesh.new()
    box(bm, (0, 0, 0.62), (1.85, 4.3, 0.55))
    box(bm, (0, -0.3, 1.14), (1.5, 2.1, 0.5))
    ob = from_bm("car", bm, rust)
    ob.location = (x, y, 0.0)
    ob.rotation_euler = (R.uniform(-0.05, 0.05), R.uniform(-0.05, 0.05), yaw)
    ob.modifiers.new("b", "BEVEL").width = 0.09
    bm = bmesh.new()
    for wx, wy in ((-0.95, 1.35), (0.95, 1.35), (-0.95, -1.35), (0.95, -1.35)):
        cyl(bm, (wx - 0.14, wy, 0.34), (wx + 0.14, wy, 0.34), 0.34, 0.34, 14)
    tyres = from_bm("wheels", bm, black)
    tyres.parent = ob
    if burning:
        f = bmesh.new()
        box(f, (0, 0, 0), (3.0, 5.0, 3.6))
        flames = from_bm("flames", f, flame_volume)
        flames.location = (x, y, 2.2)  # Object space is centered on the fire, for the falloff.
        light("POINT", (x, y, 2.6), 170, (1.0, 0.3, 0.05), size=1.0)


car(-6.2, 15, 0.5, burning=True)
car(7.2, 26, -0.35)
car(-5.3, 42, 2.8)
car(5.2, 57, 0.8, burning=True)
car(-3.4, 70, 1.4)

for y in (18.0, 46.0, 74.0):
    for side in (-1, 1):
        bm = bmesh.new()
        lean = R.uniform(-0.5, 0.5)
        cyl(bm, (side * 9, y, 0), (side * 9 + lean, y, 7.2), 0.1, 0.06, 8)
        cyl(bm, (side * 9 + lean, y, 7.2), (side * 7 + lean, y, 7.5), 0.05, 0.05, 6)
        from_bm("pole", bm, black)
        if R.random() < 0.5:
            bm = bmesh.new()
            box(bm, (side * 7 + lean, y, 7.42), (0.5, 0.22, 0.06))
            from_bm("lamp", bm, lampmat)
            light("POINT", (side * 7 + lean, y, 7.2), 240, (1.0, 0.6, 0.28), size=0.4)

# ------------------------------------------------------------------ the reactor
RX, RY = 3.5, 118.0
bm = bmesh.new()
box(bm, (RX, RY, 2.0), (24, 24, 4))
box(bm, (RX, RY, 5.5), (17, 17, 3))
for dx, dy in ((-9, -9), (9, -9), (-9, 9), (9, 9)):
    cyl(bm, (RX + dx, RY + dy, 0), (RX + dx * 0.42, RY + dy * 0.42, 31), 1.9, 0.7, 10)
cyl(bm, (RX, RY, 30), (RX, RY, 34), 8.5, 4.5, 28)
from_bm("reactor", bm, steel)
bm = bmesh.new()
cyl(bm, (RX, RY, 6), (RX, RY, 31), 3.4, 2.6, 40)
from_bm("reactor core", bm, core_glow)
bm = bmesh.new()
cyl(bm, (RX, RY, 34), (RX, RY, 400), 0.8, 2.8, 24)
from_bm("reactor beam", bm, beam_glow)
light("POINT", (RX, RY - 16, 14), 32000, (0.13, 0.75, 1.0), size=5)
light("POINT", (RX, RY - 40, 5), 11000, (0.13, 0.7, 1.0), size=3)
light("POINT", (RX, RY, 60), 30000, (0.2, 0.8, 1.0), size=6)

# ------------------------------------------------------------------ the horde
def humanoid(seed):
    """Skeleton-and-skin figure with a shambling, arms-out pose."""
    r = random.Random(seed)
    lean = r.uniform(0.1, 0.42)
    sway = r.uniform(-0.12, 0.12)
    head_t = (r.uniform(-0.13, 0.13), lean * 1.15 + r.uniform(-0.05, 0.14))
    step = r.uniform(-0.3, 0.3)
    vs = [
        (sway * 0.3, 0.0, 0.96),                                                 # 0 pelvis
        (sway * 0.7, lean * 0.45, 1.25),                                         # 1 chest
        (sway, lean * 0.9, 1.5),                                                 # 2 neck
        (sway + head_t[0], head_t[1] + lean * 0.3, 1.66),                        # 3 head
        (-0.14, step * 0.1, 0.9), (-0.15, 0.1 + step * 0.5, 0.5), (-0.16, step, 0.06),   # 4-6 left leg
        (0.14, -step * 0.1, 0.9), (0.15, 0.1 - step * 0.5, 0.5), (0.16, -step, 0.06),    # 7-9 right leg
        (sway - 0.22, lean * 0.85, 1.43),                                        # 10 left shoulder
        (sway + 0.22, lean * 0.85, 1.43),                                        # 11 right shoulder
    ]
    # Each arm reaches, hangs or is raised to claw.
    for sh, sgn in ((10, -1), (11, 1)):
        reach = r.choice((0.62, 0.62, 0.15, 0.85))
        lift = r.uniform(-0.1, 0.26) if reach > 0.3 else -0.55
        s = vs[sh]
        vs.append((s[0] + sgn * 0.05, s[1] + reach * 0.5, s[2] + lift * 0.5 - 0.08))
        vs.append((s[0] + sgn * 0.08, s[1] + reach, s[2] + lift + r.uniform(-0.1, 0.1) - 0.12))
    edges = [(0, 1), (1, 2), (2, 3), (0, 4), (4, 5), (5, 6), (0, 7), (7, 8), (8, 9), (2, 10), (2, 11), (10, 12), (12, 13), (11, 14), (14, 15)]
    radii = [0.15, 0.17, 0.065, 0.105, 0.09, 0.062, 0.05, 0.09, 0.062, 0.05, 0.055, 0.055, 0.042, 0.03, 0.042, 0.03]
    me = bpy.data.meshes.new("h")
    me.from_pydata(vs, edges, [])
    ob = bpy.data.objects.new("h", me)
    scene.collection.objects.link(ob)
    ob.modifiers.new("skin", "SKIN")
    layer = me.skin_vertices[0] if len(me.skin_vertices) else me.skin_vertices.new()
    for i, rad in enumerate(radii):
        layer.data[i].radius = (rad * r.uniform(0.9, 1.15), rad)
    layer.data[0].use_root = True
    ob.modifiers.new("ss", "SUBSURF").levels = 1
    dg = bpy.context.evaluated_depsgraph_get()
    baked = bpy.data.meshes.new_from_object(ob.evaluated_get(dg))
    bpy.data.objects.remove(ob)
    for poly in baked.polygons:
        poly.use_smooth = True
    baked.materials.append(cloth)
    baked.materials.append(flesh)
    # The body wears cloth; everything above the collar and past the elbow is skin.
    for poly in baked.polygons:
        c = poly.center
        poly.material_index = 1 if (c.z > 1.54 or abs(c.x) > 0.34 or c.y > lean * 0.9 + 0.32) else 0
    head = Vector(vs[3])
    return baked, head


variants = [humanoid(i) for i in range(10)]
eye_mesh = bmesh.new()
bmesh.ops.create_icosphere(eye_mesh, subdivisions=1, radius=1.0)
eye_me = bpy.data.meshes.new("eye")
eye_mesh.to_mesh(eye_me)
eye_mesh.free()
eye_me.materials.append(eye)


def zombie(x, y, scale=1.0, face_jitter=0.5):
    # The thumbnail camera stands further up the street: nobody should block its lens.
    if MODE == "thumb" and y < 12.5:
        return
    me, head = R.choice(variants)
    ob = bpy.data.objects.new("z", me)
    scene.collection.objects.link(ob)
    ob.location = (x, y, 0)
    # Models face +Y; the crowd walks toward the camera (-Y).
    yaw = math.pi + R.uniform(-face_jitter, face_jitter)
    ob.rotation_euler = (0, 0, yaw)
    ob.scale = (scale, scale, scale * R.uniform(0.94, 1.06))
    for sx in (-0.042, 0.042):
        e = bpy.data.objects.new("e", eye_me)
        e.parent = ob
        e.location = head + Vector((sx, 0.09, 0.015))
        e.scale = (0.016, 0.012, 0.012)
        scene.collection.objects.link(e)


# Density rises into the distance so the street reads as an endless wall of bodies.
for _ in range(36):
    zombie(R.uniform(-4.0, 8.0), R.uniform(13, 26))
for _ in range(95):
    zombie(R.uniform(-8.0, 8.5), R.uniform(24, 52))
for _ in range(300):
    zombie(R.uniform(-9.5, 9.5), R.uniform(52, 100))
# A brute leading from the front.
zombie(4.6, 13.0, 1.42, 0.2)
zombie(0.2, 11.6, 1.0, 0.3)
zombie(-2.6, 13.2, 1.05, 0.3)
zombie(7.0, 15.0, 1.0, 0.4)
zombie(-4.8, 16.5, 1.0, 0.4)

# Dead trees punctuate the kerb.
def tree(x, y):
    bm = bmesh.new()
    def branch(p, d, l, r, depth):
        q = p + d.normalized() * l
        cyl(bm, p, q, r, r * 0.68, 6)
        if depth > 0:
            for _ in range(R.randint(2, 3)):
                nd = d.normalized() + Vector((R.uniform(-0.8, 0.8), R.uniform(-0.8, 0.8), R.uniform(-0.1, 0.7)))
                branch(q, nd, l * R.uniform(0.6, 0.8), r * 0.62, depth - 1)
    branch(Vector((x, y, 0)), Vector((0, 0, 1)), 2.6, 0.2, 4)
    from_bm("tree", bm, tree_mat)


tree(-9.8, 12)
tree(10.4, 33)
tree(-10.2, 61)

# ------------------------------------------------------------------ foreground: the last line
bm = bmesh.new()
for i, x in enumerate((0.9, 3.0, 5.1, 7.2)):
    box(bm, (x, 7.0 - x * 0.1, 0.55), (2.1, 0.28, 1.1), (0, 0, -0.1))
from_bm("barricade", bm, steel)
bm = bmesh.new()
for i, x in enumerate((0.9, 3.0, 5.1, 7.2)):
    for k in range(-2, 3):
        box(bm, (x + k * 0.42, 6.84 - x * 0.1 - k * 0.042, 0.82), (0.2, 0.05, 0.3), (0, math.radians(35), -0.1))
from_bm("hazard", bm, hazard)
OX, OY = -2.6, 1.3  # Sentry placement relative to its modelled position.
# Sentry on the barricade, looking at the horde.
bm = bmesh.new()
cyl(bm, (OX + 5.2, OY + 5.2, 0), (OX + 5.2, OY + 5.2, 1.6), 0.17, 0.12, 12)
box(bm, (OX + 5.2, OY + 5.2, 1.85), (0.68, 0.95, 0.5), (0, 0, 0.18))
cyl(bm, (OX + 5.3, OY + 5.5, 1.9), (OX + 5.72, OY + 7.7, 1.93), 0.05, 0.04, 8)
cyl(bm, (OX + 5.05, OY + 5.5, 1.9), (OX + 5.47, OY + 7.7, 1.93), 0.05, 0.04, 8)
from_bm("sentry", bm, steel)
bm = bmesh.new()
box(bm, (OX + 5.45, OY + 5.73, 2.04), (0.2, 0.03, 0.05), (0, 0, 0.18))
from_bm("sentry lamp", bm, material("g", (0, 0, 0), 0.5, emit=(0.15, 1.0, 0.35), strength=45))
# Muzzle flash and its tracer stream into the crowd.
bm = bmesh.new()
for a in range(6):
    t_ = a / 6 * math.tau
    cyl(bm, (OX + 5.72, OY + 7.75, 1.93), (OX + 5.72 + math.cos(t_) * 0.45, OY + 7.75 + 0.25, 1.93 + math.sin(t_) * 0.45), 0.05, 0.0, 5)
cyl(bm, (OX + 5.72, OY + 7.75, 1.93), (OX + 5.9, OY + 9.6, 1.93), 0.09, 0.0, 6)
from_bm("flash", bm, material("mf", (0, 0, 0), 0.5, emit=(1.0, 0.58, 0.18), strength=60))
light("POINT", (OX + 5.72, OY + 8.2, 1.95), 220, (1.0, 0.55, 0.2), size=0.15)
bm = bmesh.new()
for k in range(3):
    a = Vector((OX + 5.9, OY + 10.5 + k * 7.3, 1.93 - k * 0.05))
    b = a + Vector((0.11, 3.2, -0.03))
    cyl(bm, a, b, 0.016, 0.016, 5)
from_bm("tracer", bm, material("tr", (0, 0, 0), 0.5, emit=(1.0, 0.68, 0.3), strength=110))

# Spent casings, and embers drifting through the frame.
g = bmesh.new()
for _ in range(70):
    p = (R.uniform(-9, 11), R.uniform(4, 30), R.uniform(0.2, 1.0) ** 2 * 7)
    s = R.uniform(0.005, 0.016)
    bmesh.ops.create_icosphere(g, subdivisions=1, radius=s, matrix=Matrix.Translation(p))
from_bm("embers", g, ember)

# Soft moon fill so the facades separate from the sky.
sun = bpy.data.lights.new("moon", "SUN")
sun.energy = 0.14
sun.color = (0.45, 0.6, 1.0)
sun.angle = math.radians(4)
s_ob = bpy.data.objects.new("moon", sun)
s_ob.rotation_euler = (math.radians(58), 0, math.radians(35))
scene.collection.objects.link(s_ob)

# ------------------------------------------------------------------ camera
cam = bpy.data.cameras.new("cam")
cam.lens = 26
cam.dof.use_dof = True
cam.dof.focus_distance = 11
cam.dof.aperture_fstop = 5.6
cam_ob = bpy.data.objects.new("cam", cam)
scene.collection.objects.link(cam_ob)
scene.camera = cam_ob
# Low, slightly tilted; pushed to the right so the left third stays dark for the menu.
cam_ob.location = (-2.2, -1.4, 1.25)
aim(cam_ob, (-3.2, 40, 6.0))
cam_ob.rotation_euler.rotate_axis("Z", math.radians(-1.6))

if MODE == "thumb":
    # A tighter, centered framing: reactor and horde behind, the logo in the middle of the shot.
    square = W == H
    cam.dof.use_dof = False
    cam.lens = 24 if square else 30
    # Past the barricade, with the horde close enough to loom under the logo.
    cam_ob.location = (1.4, 9.7, 1.2 if square else 1.3)
    # The square frame has spare height, so tilt down and let the reactor fill the space above the logo.
    aim(cam_ob, (2.6, 70, 7.5 if square else 9.5))
    # Bright streaks would slash across the lettering.
    for ob in bpy.data.objects:
        if ob.name.startswith(("flash", "tracer")):
            ob.hide_render = True
    # The logo rides in front of the lens, as real geometry lit by real lights.
    logo = rust_text("ROTFALL")
    logo.parent = cam_ob
    dist = 3.0
    frame = 2.0 * dist * 18.0 / cam.lens
    k = frame * (0.88 if square else 0.8) / logo.dimensions.x
    logo.scale = (k, k, k)
    logo.location = (0, 0 if not square else 0.0, -dist)
    for pos, power, colour, size in (((-1.2, 1.0, -1.2), 75, (1.0, 0.72, 0.42), 1.5), ((1.3, -0.8, -1.5), 14, (0.2, 0.85, 1.0), 1.2), ((0, 1.2, -4.2), 90, (1.0, 0.14, 0.05), 2.0)):
        l = light("AREA", pos, power, colour, size=size)
        l.parent = cam_ob
        l.rotation_euler = (Vector((0, 0, -dist)) - Vector(pos)).to_track_quat("-Z", "Y").to_euler()

scene.cycles.volume_step_rate = 4.0
bpy.ops.render.render(write_still=True)
