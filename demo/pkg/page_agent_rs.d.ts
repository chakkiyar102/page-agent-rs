/* tslint:disable */
/* eslint-disable */

export class PageAgent {
    free(): void;
    [Symbol.dispose](): void;
    /**
     * Run a task to completion. Resolves to `{ success, data }`.
     */
    execute(task: string): Promise<any>;
    /**
     * Current page as the model sees it (handy for debugging prompts).
     */
    getBrowserState(): string;
    /**
     * Floating console, bottom-right, in a shadow root. Excluded from the agent's own view.
     */
    mountPanel(): void;
    constructor(config: any);
    /**
     * `fn({tool, args, index, label, element}) -> boolean | Promise<boolean>` awaited before every
     * action (not `done`). `false`, a throw or a rejection skips the action and tells the model the
     * user declined it; anything else proceeds. `element` is the target DOM node, or null.
     * Pass `null` to remove the hook.
     */
    onBeforeAction(f?: Function | null): void;
    /**
     * `fn(event: {type, ...})` called for task/thinking/step/executed/done/error.
     */
    onEvent(f: Function): void;
    /**
     * Replace the config (e.g. after the user pastes a key). Applies to the next task.
     */
    setConfig(config: any): void;
    /**
     * Abort after the current step.
     */
    stop(): void;
    readonly running: boolean;
}

export type InitInput = RequestInfo | URL | Response | BufferSource | WebAssembly.Module;

export interface InitOutput {
    readonly memory: WebAssembly.Memory;
    readonly __wbg_pageagent_free: (a: number, b: number) => void;
    readonly pageagent_execute: (a: number, b: number, c: number) => any;
    readonly pageagent_getBrowserState: (a: number) => [number, number];
    readonly pageagent_mountPanel: (a: number) => [number, number];
    readonly pageagent_new: (a: any) => [number, number, number];
    readonly pageagent_onBeforeAction: (a: number, b: number) => void;
    readonly pageagent_onEvent: (a: number, b: any) => void;
    readonly pageagent_running: (a: number) => number;
    readonly pageagent_setConfig: (a: number, b: any) => [number, number];
    readonly pageagent_stop: (a: number) => void;
    readonly wasm_bindgen_e666d8eafe4949ee___convert__closures_____invoke___js_sys_2bb0207fd6764501___Function_fn_wasm_bindgen_e666d8eafe4949ee___JsValue_____wasm_bindgen_e666d8eafe4949ee___sys__Undefined___js_sys_2bb0207fd6764501___Function_fn_wasm_bindgen_e666d8eafe4949ee___JsValue_____wasm_bindgen_e666d8eafe4949ee___sys__Undefined_______true_: (a: number, b: number, c: any, d: any) => void;
    readonly wasm_bindgen_e666d8eafe4949ee___convert__closures_____invoke___wasm_bindgen_e666d8eafe4949ee___JsValue__core_ed718c3d60ebd546___result__Result_____wasm_bindgen_e666d8eafe4949ee___JsError___true_: (a: number, b: number, c: any) => [number, number];
    readonly wasm_bindgen_e666d8eafe4949ee___convert__closures_____invoke___web_sys_60075e0863155b4b___features__gen_Event__Event______true_: (a: number, b: number, c: any) => void;
    readonly __wbindgen_malloc: (a: number, b: number) => number;
    readonly __wbindgen_realloc: (a: number, b: number, c: number, d: number) => number;
    readonly __wbindgen_exn_store: (a: number) => void;
    readonly __externref_table_alloc: () => number;
    readonly __wbindgen_externrefs: WebAssembly.Table;
    readonly __wbindgen_destroy_closure: (a: number, b: number) => void;
    readonly __wbindgen_free: (a: number, b: number, c: number) => void;
    readonly __externref_table_dealloc: (a: number) => void;
    readonly __wbindgen_start: () => void;
}

export type SyncInitInput = BufferSource | WebAssembly.Module;

/**
 * Instantiates the given `module`, which can either be bytes or
 * a precompiled `WebAssembly.Module`.
 *
 * @param {{ module: SyncInitInput }} module - Passing `SyncInitInput` directly is deprecated.
 *
 * @returns {InitOutput}
 */
export function initSync(module: { module: SyncInitInput } | SyncInitInput): InitOutput;

/**
 * If `module_or_path` is {RequestInfo} or {URL}, makes a request and
 * for everything else, calls `WebAssembly.instantiate` directly.
 *
 * @param {{ module_or_path: InitInput | Promise<InitInput> }} module_or_path - Passing `InitInput` directly is deprecated.
 *
 * @returns {Promise<InitOutput>}
 */
export default function __wbg_init (module_or_path?: { module_or_path: InitInput | Promise<InitInput> } | InitInput | Promise<InitInput>): Promise<InitOutput>;
