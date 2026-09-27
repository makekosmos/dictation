import { existsSync } from "node:fs";
import { spawnSync } from "node:child_process";

const executable = `worker/dictation-worker.exe`;
if (!existsSync(executable)) throw new Error(`missing worker: ${executable}`);

const result = spawnSync(executable, {
  input: `${JSON.stringify({ method: "worker.bootstrap", token: "smoke-token", generation: 3 })}\n`,
  encoding: "utf8",
  timeout: 5000,
  windowsHide: true,
});
if (result.error) throw result.error;
if (result.status !== 0)
  throw new Error(`worker exited with status ${result.status}: ${result.stderr}`);
const hello = result.stdout
  .trim()
  .split(/\r?\n/)
  .filter(Boolean)
  .map((line) => JSON.parse(line))
  .find((message) => message.method === "worker.hello");
if (!hello || hello.token !== "smoke-token")
  throw new Error("worker did not emit a valid worker.hello");
console.log("worker ok: worker.hello");
