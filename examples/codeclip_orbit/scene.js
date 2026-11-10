// kimo code clip example (MIT, part of kimo). A lit torus knot inside a ring of cubes, camera orbiting.
// Contract: window.renderFrame(t, params) draws the exact frame for t seconds. Everything is a closed-form function of
// (t, params): no clock, no Math.random, no state carried between frames. Transparent background, so Kimo can composite it.
import * as THREE from 'three';

const W = window.innerWidth, H = window.innerHeight;
const renderer = new THREE.WebGLRenderer({ antialias: false, alpha: true, preserveDrawingBuffer: true });
renderer.setPixelRatio(1);
renderer.setSize(W, H);
renderer.setClearColor(0x000000, 0);
document.body.appendChild(renderer.domElement);

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(45, W / H, 0.1, 100);
scene.add(new THREE.HemisphereLight(0xffffff, 0x223344, 1.2));
const key = new THREE.DirectionalLight(0xffffff, 2.0);
key.position.set(3, 5, 4);
scene.add(key);

const knotMat = new THREE.MeshStandardMaterial({ roughness: 0.35, metalness: 0.2 });
const knot = new THREE.Mesh(new THREE.TorusKnotGeometry(0.8, 0.26, 160, 24), knotMat);
scene.add(knot);

// Ring of cubes; positions come from a fixed hash, so every page load builds the same ring.
const hash = (i) => { let x = Math.imul(i + 1, 0x9E3779B1) >>> 0; x ^= x >>> 15; x = Math.imul(x, 0x85EBCA77) >>> 0; x ^= x >>> 13; return (x >>> 0) / 4294967296; };
const cubes = [];
const cubeGeo = new THREE.BoxGeometry(0.22, 0.22, 0.22);
for (let i = 0; i < 24; i++) {
  const m = new THREE.Mesh(cubeGeo, new THREE.MeshStandardMaterial({ roughness: 0.5 }));
  m.userData = { a: (i / 24) * Math.PI * 2, r: 2.0 + 0.35 * hash(i), y: (hash(i + 99) - 0.5) * 0.8, s: 0.6 + hash(i + 7) };
  cubes.push(m); scene.add(m);
}

window.renderFrame = (t, params) => {
  const p = Object.assign({ hue: 0.58, speed: 1.0, orbit: 0.0 }, params || {});
  const s = t * p.speed;
  const ang = p.orbit + s * 0.6;
  camera.position.set(Math.sin(ang) * 6, 1.6 + Math.sin(s * 0.5) * 0.4, Math.cos(ang) * 6);
  camera.lookAt(0, 0, 0);
  knot.rotation.set(s * 0.7, s * 0.45, 0);
  knotMat.color.setHSL(p.hue, 0.75, 0.55);
  for (const m of cubes) {
    const u = m.userData, a = u.a + s * 0.8;
    m.position.set(Math.cos(a) * u.r, u.y + Math.sin(s * 2 + u.a * 3) * 0.15, Math.sin(a) * u.r);
    m.rotation.set(s * u.s, s * 0.7 * u.s, 0);
    m.material.color.setHSL((p.hue + 0.5 + u.a / 12) % 1, 0.7, 0.6);
  }
  renderer.render(scene, camera);
};
window.__ready = true;
