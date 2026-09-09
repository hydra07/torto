declare module "../pkg/rebook_engine_wasm.js" {
  const init: () => Promise<unknown>;
  export default init;
  export class WebReader {
    constructor();
    static create(canvas: HTMLCanvasElement): Promise<WebReader>;
    open_bytes(bytes: Uint8Array, fileName: string, width: number, height: number): string;
    page_info(): string;
    page_text(): string;
    navigate_next(): number;
    navigate_previous(): number;
    resize(width: number, height: number): void;
    toc_json(): string;
  }
}
