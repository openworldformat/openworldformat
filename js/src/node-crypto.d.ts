// node:crypto, declared by hand: this package deliberately ships no
// @types/node (index.js's only Node API is the sha256 behind entry
// identity, and render.js must keep typechecking without Node's typings
// in scope). Declared to the shape the fold uses, nothing more.
declare module "node:crypto" {
  export interface Hash {
    update(data: string, encoding: string): Hash;
    digest(encoding: "hex"): string;
  }
  export function createHash(algorithm: "sha256"): Hash;
}
