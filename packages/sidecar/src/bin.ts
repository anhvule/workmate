/**
 * Process entry point.
 *
 * Separate from `main.ts` so importing the message loop for a test does not
 * start it: a module that boots on import cannot be tested honestly.
 */
import { run } from "./main.js";

run();
