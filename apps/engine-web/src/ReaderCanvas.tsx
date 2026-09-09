import { useEffect, useRef, useState } from "react";

type Status = "checking" | "ready" | "failed";
type TocItem = { id: string; label: string; depth: number };
type ReaderState = {
    progression: number;
    active_toc_id: string | null;
    active_toc_path: string[];
    location: { page_index: number; page_count: number };
};
type SelectionState = {
    text: string;
    rects: Array<{ x: number; y: number; width: number; height: number }>;
};

const parseJson = <T,>(value: unknown): T => JSON.parse(String(value)) as T;

export function ReaderCanvas({
    bytes,
    fileName,
}: {
    bytes?: Uint8Array;
    fileName: string;
}) {
    const ref = useRef<HTMLCanvasElement>(null);
    const [status, setStatus] = useState<Status>("checking");
    const [engineStatus, setEngineStatus] = useState("Engine idle");
    const [rendererKind, setRendererKind] = useState<"webgpu" | "cpu">();
    const [toc, setToc] = useState<TocItem[]>([]);
    const [readerState, setReaderState] = useState<ReaderState>();
    const [tocOpen, setTocOpen] = useState(false);
    const [selectedText, setSelectedText] = useState("");
    const [selection, setSelection] = useState<SelectionState>({
        text: "",
        rects: [],
    });
    const readerRef = useRef<any>(null);
    const navigatingRef = useRef(false);
    const claimedPointerRef = useRef<number | undefined>(undefined);
    const selectionPointerRef = useRef<number | undefined>(undefined);
    const longPressTimerRef = useRef<number | undefined>(undefined);
    const transitionFrameRef = useRef<number | undefined>(undefined);
    const selectionFrameRef = useRef<number | undefined>(undefined);
    const selectionPointRef = useRef<{ x: number; y: number } | undefined>(
        undefined,
    );
    const pointerOriginRef = useRef<
        { id: number; x: number; y: number } | undefined
    >(undefined);
    const suppressClickRef = useRef(false);

    const locatorStorageKey = bytes
        ? `torto:locator:${fileName}:${bytes.byteLength}`
        : undefined;

    const syncReaderState = (reader: any) => {
        const next = parseJson<ReaderState>(reader.state_json());
        setReaderState(next);
        if (locatorStorageKey) {
            localStorage.setItem(
                locatorStorageKey,
                String(reader.locator_json()),
            );
        }
    };

    const syncSelection = (reader: any) => {
        const next = parseJson<SelectionState>(reader.selection_json());
        setSelection(next);
        setSelectedText(next.text);
    };

    const scheduleSelectionUpdate = (reader: any, x: number, y: number) => {
        selectionPointRef.current = { x, y };
        if (selectionFrameRef.current !== undefined) return;
        selectionFrameRef.current = requestAnimationFrame(() => {
            selectionFrameRef.current = undefined;
            const point = selectionPointRef.current;
            if (!point || readerRef.current !== reader) return;
            try {
                reader.selection_update(point.x, point.y);
                reader.render_frame();
                syncSelection(reader);
            } catch (error) {
                console.error("Selection update failed:", error);
            }
        });
    };

    const scheduleTransitionFrame = () => {
        if (transitionFrameRef.current !== undefined) return;
        transitionFrameRef.current = requestAnimationFrame(
            function animate(timestamp) {
                transitionFrameRef.current = undefined;
                const reader = readerRef.current;
                if (!reader) return;
                try {
                    reader.tick(4.0);
                    const state = reader.animation_step(timestamp);
                    reader.render_frame();
                    if (state === 2) syncReaderState(reader);
                    if (state === 1) {
                        transitionFrameRef.current =
                            requestAnimationFrame(animate);
                    }
                } catch (error) {
                    console.error("Page curl animation failed:", error);
                    setEngineStatus(
                        `Animation error: ${error instanceof Error ? error.message : String(error)}`,
                    );
                }
            },
        );
    };

    const navigate = async (direction: "next" | "previous") => {
        const reader = readerRef.current;
        if (!reader || navigatingRef.current) return;
        navigatingRef.current = true;
        setSelectedText("");
        setSelection({ text: "", rects: [] });
        try {
            let state = 0;
            for (;;) {
                state =
                    direction === "next"
                        ? reader.navigate_next()
                        : reader.navigate_previous();
                if (state !== 1) break;
                reader.tick(12.0);
                await new Promise<void>((resolve) =>
                    requestAnimationFrame(() => resolve()),
                );
            }
            reader.render_frame();
            syncReaderState(reader);
            setEngineStatus(
                state === 0
                    ? `Reached the ${direction === "next" ? "end" : "beginning"} of the book`
                    : `Moved ${direction}`,
            );
        } catch (error) {
            console.error("Navigation render failed:", error);
            setEngineStatus(
                `Navigation error: ${error instanceof Error ? error.message : String(error)}`,
            );
        } finally {
            navigatingRef.current = false;
        }
    };

    useEffect(() => {
        setStatus("checking");
        setSelectedText("");
        setSelection({ text: "", rects: [] });
    }, []);

    useEffect(() => {
        if (!bytes || !ref.current) return;
        let alive = true;
        let observer: ResizeObserver | undefined;
        let resizeFrame = 0;

        (async () => {
            try {
                const wasm = await import("../pkg/rebook_engine_wasm.js");
                await wasm.default();
                if (!alive || !ref.current) return;

                const rect = ref.current.getBoundingClientRect();
                const logicalWidth = Math.max(
                    1,
                    Math.round(rect.width || 1200),
                );
                const logicalHeight = Math.max(
                    1,
                    Math.round(rect.height || 760),
                );

                // Renderer and layout currently share logical-pixel geometry.
                // Configure the backing store before selecting GPU/CPU so a
                // failed WebGPU probe leaves a correctly-sized Canvas2D target.
                ref.current.width = logicalWidth;
                ref.current.height = logicalHeight;

                const reader = await wasm.WebReader.create(ref.current);
                if (!alive) {
                    reader.close();
                    reader.free();
                    return;
                }
                readerRef.current = reader;
                setRendererKind(
                    reader.renderer_kind() === "cpu" ? "cpu" : "webgpu",
                );
                setStatus("ready");

                const result = reader.open_bytes(
                    bytes,
                    fileName,
                    logicalWidth,
                    logicalHeight,
                );
                if (locatorStorageKey) {
                    const savedLocator =
                        localStorage.getItem(locatorStorageKey);
                    if (savedLocator) {
                        try {
                            reader.restore_locator_json(savedLocator);
                        } catch (error) {
                            localStorage.removeItem(locatorStorageKey);
                            console.warn(
                                "Saved reading position was discarded:",
                                error,
                            );
                        }
                    }
                }
                const info = reader.page_info();

                reader.render_frame();
                setToc(parseJson<TocItem[]>(reader.toc_json()));
                syncReaderState(reader);

                // The browser owns scheduling only; parsing/pagination work and
                // its priority remain engine-controlled. Small RAF slices warm
                // the adjacent spreads without blocking initial paint.
                const warmAdjacent = () => {
                    if (
                        !alive ||
                        readerRef.current !== reader ||
                        navigatingRef.current
                    )
                        return;
                    try {
                        if (reader.tick(4.0) === 1) {
                            requestAnimationFrame(warmAdjacent);
                        }
                    } catch (error) {
                        console.error("Engine prefetch failed:", error);
                    }
                };
                requestAnimationFrame(warmAdjacent);

                if (alive) {
                    setEngineStatus(`Engine opened: ${result} · page: ${info}`);
                }

                observer = new ResizeObserver(() => {
                    cancelAnimationFrame(resizeFrame);
                    resizeFrame = requestAnimationFrame(() => {
                        const next = ref.current?.getBoundingClientRect();
                        if (!next || !alive || !readerRef.current) return;
                        const lWidth = Math.max(1, Math.round(next.width));
                        const lHeight = Math.max(1, Math.round(next.height));
                        if (ref.current) {
                            ref.current.width = lWidth;
                            ref.current.height = lHeight;
                        }
                        readerRef.current.resize(lWidth, lHeight);
                        try {
                            readerRef.current.render_frame();
                            setSelectedText("");
                            setSelection({ text: "", rects: [] });
                            syncReaderState(readerRef.current);
                        } catch (e) {
                            console.error("Render after resize failed:", e);
                        }
                    });
                });
                if (ref.current) observer.observe(ref.current);
            } catch (error) {
                if (alive) {
                    setStatus("failed");
                    setEngineStatus(
                        `Engine error: ${error instanceof Error ? error.message : String(error)}`,
                    );
                }
            }
        })();

        return () => {
            alive = false;
            if (longPressTimerRef.current !== undefined) {
                window.clearTimeout(longPressTimerRef.current);
                longPressTimerRef.current = undefined;
            }
            if (transitionFrameRef.current !== undefined) {
                cancelAnimationFrame(transitionFrameRef.current);
                transitionFrameRef.current = undefined;
            }
            if (selectionFrameRef.current !== undefined) {
                cancelAnimationFrame(selectionFrameRef.current);
                selectionFrameRef.current = undefined;
            }
            observer?.disconnect();
            cancelAnimationFrame(resizeFrame);
            if (readerRef.current) {
                readerRef.current.close();
                readerRef.current.free();
                readerRef.current = null;
            }
        };
    }, [bytes, fileName, locatorStorageKey]);

    const navigateToc = (id: string) => {
        const reader = readerRef.current;
        if (!reader || navigatingRef.current) return;
        try {
            reader.navigate_toc(id);
            reader.render_frame();
            syncReaderState(reader);
            setSelectedText("");
            setSelection({ text: "", rects: [] });
            setTocOpen(false);
        } catch (error) {
            console.error("TOC navigation failed:", error);
        }
    };

    return (
        <div
            className="canvas-wrap"
            tabIndex={0}
            role="application"
            aria-label="Ebook reader"
            onKeyDown={(event) => {
                if (
                    event.key === "ArrowRight" ||
                    event.key === " " ||
                    event.key === "PageDown"
                ) {
                    event.preventDefault();
                    void navigate("next");
                } else if (
                    event.key === "ArrowLeft" ||
                    event.key === "PageUp"
                ) {
                    event.preventDefault();
                    void navigate("previous");
                }
            }}
            onPointerDown={(event) => {
                if (
                    !event.isPrimary ||
                    (event.target as HTMLElement).closest("button, nav")
                )
                    return;
                const reader = readerRef.current;
                if (!reader) return;
                event.currentTarget.focus({ preventScroll: true });
                const canvasBounds = ref.current?.getBoundingClientRect();
                if (!canvasBounds) return;
                const x = event.clientX - canvasBounds.left;
                const y = event.clientY - canvasBounds.top;

                if (event.pointerType === "mouse" && event.button === 0) {
                    if (reader.selection_start(x, y)) {
                        syncSelection(reader);
                        selectionPointerRef.current = event.pointerId;
                        suppressClickRef.current = true;
                        event.currentTarget.setPointerCapture(event.pointerId);
                        reader.render_frame();
                        event.preventDefault();
                        return;
                    }
                    setSelectedText("");
                    setSelection({ text: "", rects: [] });
                    reader.render_frame();
                }

                reader.pointer_down(
                    event.pointerId,
                    event.clientX,
                    event.clientY,
                    event.timeStamp,
                );
                pointerOriginRef.current = {
                    id: event.pointerId,
                    x: event.clientX,
                    y: event.clientY,
                };
                if (event.pointerType !== "mouse") {
                    const target = event.currentTarget;
                    longPressTimerRef.current = window.setTimeout(() => {
                        longPressTimerRef.current = undefined;
                        if (
                            pointerOriginRef.current?.id !== event.pointerId ||
                            !readerRef.current
                        )
                            return;
                        readerRef.current.pointer_cancel(performance.now());
                        if (readerRef.current.selection_start(x, y)) {
                            syncSelection(readerRef.current);
                            selectionPointerRef.current = event.pointerId;
                            suppressClickRef.current = true;
                            target.setPointerCapture(event.pointerId);
                            readerRef.current.render_frame();
                        }
                    }, 450);
                }
            }}
            onPointerMove={(event) => {
                const reader = readerRef.current;
                if (!reader || !event.isPrimary) return;
                const canvasBounds = ref.current?.getBoundingClientRect();
                if (
                    selectionPointerRef.current === event.pointerId &&
                    canvasBounds
                ) {
                    scheduleSelectionUpdate(
                        reader,
                        event.clientX - canvasBounds.left,
                        event.clientY - canvasBounds.top,
                    );
                    event.preventDefault();
                    return;
                }
                const origin = pointerOriginRef.current;
                if (
                    origin?.id === event.pointerId &&
                    Math.hypot(
                        event.clientX - origin.x,
                        event.clientY - origin.y,
                    ) > 8 &&
                    longPressTimerRef.current !== undefined
                ) {
                    window.clearTimeout(longPressTimerRef.current);
                    longPressTimerRef.current = undefined;
                }
                const result = reader.pointer_move(
                    event.pointerId,
                    event.clientX,
                    event.clientY,
                    event.timeStamp,
                );
                if (result === 2) {
                    claimedPointerRef.current = event.pointerId;
                    suppressClickRef.current = true;
                    event.currentTarget.setPointerCapture(event.pointerId);
                    reader.render_frame();
                    scheduleTransitionFrame();
                    event.preventDefault();
                }
            }}
            onPointerUp={(event) => {
                const reader = readerRef.current;
                if (!reader || !event.isPrimary) return;
                const canvasBounds = ref.current?.getBoundingClientRect();
                if (longPressTimerRef.current !== undefined) {
                    window.clearTimeout(longPressTimerRef.current);
                    longPressTimerRef.current = undefined;
                }
                pointerOriginRef.current = undefined;
                if (selectionPointerRef.current === event.pointerId) {
                    selectionPointerRef.current = undefined;
                    if (canvasBounds) {
                        reader.selection_update(
                            event.clientX - canvasBounds.left,
                            event.clientY - canvasBounds.top,
                        );
                    }
                    setSelectedText(String(reader.selection_end()));
                    syncSelection(reader);
                    if (
                        event.currentTarget.hasPointerCapture(event.pointerId)
                    ) {
                        event.currentTarget.releasePointerCapture(
                            event.pointerId,
                        );
                    }
                    event.preventDefault();
                    window.setTimeout(() => {
                        suppressClickRef.current = false;
                    }, 0);
                    return;
                }
                const result = reader.pointer_up(
                    event.pointerId,
                    event.clientX,
                    event.clientY,
                    event.timeStamp,
                );
                if (result === 2) scheduleTransitionFrame();
                if (claimedPointerRef.current === event.pointerId) {
                    claimedPointerRef.current = undefined;
                    if (
                        event.currentTarget.hasPointerCapture(event.pointerId)
                    ) {
                        event.currentTarget.releasePointerCapture(
                            event.pointerId,
                        );
                    }
                    event.preventDefault();
                    window.setTimeout(() => {
                        suppressClickRef.current = false;
                    }, 0);
                }
                if (result === 3) void navigate("next");
                if (result === 4) void navigate("previous");
            }}
            onPointerCancel={(event) => {
                if (longPressTimerRef.current !== undefined) {
                    window.clearTimeout(longPressTimerRef.current);
                    longPressTimerRef.current = undefined;
                }
                pointerOriginRef.current = undefined;
                if (selectionPointerRef.current === event.pointerId) {
                    selectionPointerRef.current = undefined;
                    setSelectedText(
                        String(readerRef.current?.selection_end() ?? ""),
                    );
                    if (readerRef.current) syncSelection(readerRef.current);
                }
                readerRef.current?.pointer_cancel(event.timeStamp);
                scheduleTransitionFrame();
                if (claimedPointerRef.current === event.pointerId) {
                    claimedPointerRef.current = undefined;
                    suppressClickRef.current = true;
                    window.setTimeout(() => {
                        suppressClickRef.current = false;
                    }, 0);
                }
            }}
            onBlur={() => {
                readerRef.current?.focus_lost(performance.now());
                scheduleTransitionFrame();
                if (selectionPointerRef.current !== undefined) {
                    setSelectedText(
                        String(readerRef.current?.selection_end() ?? ""),
                    );
                    if (readerRef.current) syncSelection(readerRef.current);
                    selectionPointerRef.current = undefined;
                }
                claimedPointerRef.current = undefined;
            }}
            onClick={(event) => {
                if ((event.target as HTMLElement).closest("button")) return;
                if (suppressClickRef.current) {
                    suppressClickRef.current = false;
                    return;
                }
                const bounds = event.currentTarget.getBoundingClientRect();
                const relativeX = event.clientX - bounds.left;
                void navigate(
                    relativeX < bounds.width / 2 ? "previous" : "next",
                );
            }}
        >
            <canvas ref={ref} width={1200} height={760} />
            <button
                className="toc-toggle"
                aria-expanded={tocOpen}
                aria-controls="reader-toc"
                onClick={() => setTocOpen((open) => !open)}
            >
                Contents
            </button>
            {tocOpen && (
                <nav
                    id="reader-toc"
                    className="toc"
                    aria-label="Table of contents"
                >
                    <strong>Contents</strong>
                    {toc.length === 0 ? (
                        <p>No table of contents</p>
                    ) : (
                        toc.map((item) => (
                            <button
                                key={item.id}
                                className={
                                    readerState?.active_toc_id === item.id
                                        ? "active"
                                        : ""
                                }
                                style={{
                                    paddingInlineStart: `${12 + item.depth * 14}px`,
                                }}
                                onClick={() => navigateToc(item.id)}
                            >
                                {item.label}
                            </button>
                        ))
                    )}
                </nav>
            )}

            <button
                className="nav prev"
                aria-label="Previous page"
                onClick={() => void navigate("previous")}
            >
                ‹
            </button>

            <button
                className="nav next"
                aria-label="Next page"
                onClick={() => void navigate("next")}
            >
                ›
            </button>
            {selectedText && (
                <div
                    className="selection-toolbar"
                    role="toolbar"
                    aria-label="Text selection"
                >
                    <button
                        onClick={async () => {
                            try {
                                await navigator.clipboard.writeText(
                                    selectedText,
                                );
                                setEngineStatus("Selected text copied");
                            } catch (error) {
                                setEngineStatus(
                                    `Copy failed: ${error instanceof Error ? error.message : String(error)}`,
                                );
                            }
                        }}
                    >
                        Copy
                    </button>
                    <button
                        aria-label="Clear selection"
                        onClick={() => {
                            readerRef.current?.selection_clear();
                            readerRef.current?.render_frame();
                            setSelectedText("");
                            setSelection({ text: "", rects: [] });
                        }}
                    >
                        ×
                    </button>
                </div>
            )}
            {readerState && (
                <div
                    className="reader-progress"
                    aria-label={`Reading progress ${Math.round(readerState.progression * 100)}%`}
                >
                    <span
                        style={{ width: `${readerState.progression * 100}%` }}
                    />
                    <small>
                        {Math.round(readerState.progression * 100)}% · page{" "}
                        {readerState.location.page_index + 1}/
                        {readerState.location.page_count}
                    </small>
                </div>
            )}
            <div className={`status ${status}`}>
                {status === "ready"
                    ? rendererKind === "cpu"
                        ? "CPU renderer active"
                        : "WebGPU renderer active"
                    : status === "checking"
                      ? "Selecting renderer…"
                      : "GPU and CPU renderers unavailable"}
                <br />
                <small>{engineStatus}</small>
            </div>
        </div>
    );
}
