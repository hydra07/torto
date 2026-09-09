declare module "../pkg/rebook_engine_wasm.js" {
  const init: () => Promise<unknown>;
  export default init;
  export class WebReader {
    constructor();
    static create(canvas: HTMLCanvasElement): Promise<WebReader>;
    renderer_kind(): string;
    open_bytes(bytes: Uint8Array, fileName: string, width: number, height: number): string;
    resize(width: number, height: number): void;
    render_frame(): void;
    tick(budgetMs: number): number;
    close(): void;
    is_open(): boolean;
    toc_json(): string;
    state_json(): string;
    locator_json(): string;
    restore_locator_json(locatorJson: string): void;
    navigate_toc(id: string): void;
    search(query: string, maxResults: number): string;
    set_highlights_json(rangesJson: string): void;
    clear_highlights(): void;
    set_focus_json(rangesJson: string): void;
    clear_focus(): void;
    style_json(): string;
    set_style_json(styleJson: string): void;
    set_font_size(fontSize: number): void;
    set_line_height(lineHeight: number): void;
    set_margins(horizontal: number, top: number, bottom: number): void;
    set_spread_mode(mode: string): void;
    set_colors(fgR: number, fgG: number, fgB: number, bgR: number, bgG: number, bgB: number): void;
    page_info(): string;
    page_text(): string;
    navigate_next(): number;
    navigate_previous(): number;
    pointer_down(id: number, x: number, y: number, timestampMs: number): number;
    pointer_move(id: number, x: number, y: number, timestampMs: number): number;
    pointer_up(id: number, x: number, y: number, timestampMs: number): number;
    pointer_cancel(timestampMs: number): number;
    focus_lost(timestampMs: number): number;
    selection_start(x: number, y: number): boolean;
    selection_update(x: number, y: number): boolean;
    selection_end(): string;
    selection_clear(): boolean;
    selection_json(): string;
    animation_step(timestampMs: number): number;
  }
}
