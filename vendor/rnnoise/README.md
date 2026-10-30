# RNNoise (WebAssembly)

Suppression de bruit utilisée par Plume (micro), chargée seulement quand
l'option est activée.

- `worklet.js` : processeur AudioWorklet de
  [@sapphi-red/web-noise-suppressor](https://github.com/sapphi-red/web-noise-suppressor)
  0.4.1 (MIT, voir `LICENSE-web-noise-suppressor`), qui embarque le chargeur de
  [@shiguredo/rnnoise-wasm](https://github.com/shiguredo/rnnoise-wasm) (Apache-2.0).
- `rnnoise_simd.wasm` : [RNNoise](https://github.com/xiph/rnnoise) (Xiph.Org,
  licence BSD 3 clauses), compilé en WebAssembly SIMD par shiguredo.
- `test.html` : banc d'essai local (servir ce dossier en HTTP et l'ouvrir) qui compare l'énergie avant/après RNNoise.
