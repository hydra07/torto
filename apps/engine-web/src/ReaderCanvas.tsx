import { useEffect, useRef, useState } from "react";

type Status = "checking" | "ready" | "failed";
type TocItem = { id: string; label: string; depth: number };
type ReaderState = {
    progression: number;
    active_toc_id: string | null;
    active_toc_path: string[];
    location: { page_index: number; page_count: number };
};
type SourceRange = {
    start: { spine: string; node: string; text_offset: number };
    end: { spine: string; node: string; text_offset: number };
};
type SelectionState = {
    text: string;
    ranges?: SourceRange[];
    rects: Array<{ x: number; y: number; width: number; height: number }>;
};
type SearchResult = {
    section_index: number;
    section_title: string;
    excerpt: string;
    matched_text: string;
    block_kind: string;
    range: SourceRange;
    locator: unknown;
};
type Bookmark = {
    id: string;
    title: string;
    locator: unknown;
    created_at_ms: number;
};

const parseJson = <T,>(value: unknown): T => JSON.parse(String(value)) as T;

export function ReaderCanvas({
    bytes,
    fileName,
    zenMode,
    onToggleZenMode,
}: {
    bytes?: Uint8Array;
    fileName: string;
    zenMode?: boolean;
    onToggleZenMode?: () => void;
}) {
    const ref = useRef<HTMLCanvasElement>(null);
    const [status, setStatus] = useState<Status>("checking");
    const [engineStatus, setEngineStatus] = useState("Engine idle");
    const [rendererKind, setRendererKind] = useState<"webgpu" | "cpu">();
    const [toc, setToc] = useState<TocItem[]>([]);
    const [readerState, setReaderState] = useState<ReaderState>();
    const [tocOpen, setTocOpen] = useState(false);
    const [searchOpen, setSearchOpen] = useState(false);
    const [bookmarksOpen, setBookmarksOpen] = useState(false);
    const [settingsOpen, setSettingsOpen] = useState(false);
    const [searchQuery, setSearchQuery] = useState("");
    const [searchResults, setSearchResults] = useState<SearchResult[]>([]);
    const [bookmarks, setBookmarks] = useState<Bookmark[]>([]);
    const [highlights, setHighlights] = useState<SourceRange[]>([]);
    const [selectedText, setSelectedText] = useState("");
    const [selection, setSelection] = useState<SelectionState>({
        text: "",
        rects: [],
    });

    // Reader appearance and typography state
    const [fontSize, setFontSize] = useState<number>(20);
    const [lineHeight, setLineHeight] = useState<number>(1.5);
    const [marginLevel, setMarginLevel] = useState<"compact" | "normal" | "wide">("normal");
    const [theme, setTheme] = useState<"light" | "sepia" | "dark" | "black">("light");
    const [spreadMode, setSpreadMode] = useState<"single" | "double" | "scroll">("double");
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
                const dpr = Math.max(1, window.devicePixelRatio || 1);
                const logicalWidth = Math.max(
                    1,
                    Math.round(rect.width || 1200),
                );
                const logicalHeight = Math.max(
                    1,
                    Math.round(rect.height || 760),
                );

                // Configure backing store with HiDPI dpr for ultra-crisp vector glyphs
                ref.current.width = Math.round(logicalWidth * dpr);
                ref.current.height = Math.round(logicalHeight * dpr);

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
                    Math.round(logicalWidth * dpr),
                    Math.round(logicalHeight * dpr),
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
                        const dpr = Math.max(1, window.devicePixelRatio || 1);
                        const lWidth = Math.max(1, Math.round(next.width));
                        const lHeight = Math.max(1, Math.round(next.height));
                        const targetWidth = Math.round(lWidth * dpr);
                        const targetHeight = Math.round(lHeight * dpr);
                        if (ref.current) {
                            ref.current.width = targetWidth;
                            ref.current.height = targetHeight;
                        }
                        readerRef.current.resize(targetWidth, targetHeight);
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

    const bookmarksStorageKey = bytes
        ? `torto:bookmarks:${fileName}:${bytes.byteLength}`
        : undefined;

    const highlightsStorageKey = bytes
        ? `torto:highlights:${fileName}:${bytes.byteLength}`
        : undefined;

    useEffect(() => {
        if (bookmarksStorageKey) {
            try {
                const saved = localStorage.getItem(bookmarksStorageKey);
                if (saved) setBookmarks(JSON.parse(saved));
            } catch (e) {
                console.error("Failed to load bookmarks:", e);
            }
        }
        if (highlightsStorageKey) {
            try {
                const saved = localStorage.getItem(highlightsStorageKey);
                if (saved) {
                    const parsed = JSON.parse(saved) as SourceRange[];
                    setHighlights(parsed);
                    if (readerRef.current) {
                        readerRef.current.set_highlights_json(JSON.stringify(parsed));
                        readerRef.current.render_frame();
                    }
                }
            } catch (e) {
                console.error("Failed to load highlights:", e);
            }
        }
    }, [bookmarksStorageKey, highlightsStorageKey]);

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

    const handleSearch = (query: string) => {
        setSearchQuery(query);
        const reader = readerRef.current;
        if (!reader || !query.trim()) {
            setSearchResults([]);
            reader?.clear_focus();
            reader?.render_frame();
            return;
        }
        try {
            const results = parseJson<SearchResult[]>(reader.search(query, 50));
            setSearchResults(results);
            const ranges = results.map((r) => r.range);
            reader.set_focus_json(JSON.stringify(ranges));
            reader.render_frame();
        } catch (error) {
            console.error("Search failed:", error);
        }
    };

    const jumpToSearchResult = (result: SearchResult) => {
        const reader = readerRef.current;
        if (!reader) return;
        try {
            reader.restore_locator_json(JSON.stringify(result.locator));
            reader.render_frame();
            syncReaderState(reader);
            setSearchOpen(false);
            setEngineStatus(`Jumped to match: "${result.matched_text}"`);
        } catch (error) {
            console.error("Jump to search result failed:", error);
        }
    };

    const toggleBookmark = () => {
        const reader = readerRef.current;
        if (!reader || !readerState) return;
        try {
            const locator = parseJson(reader.locator_json());
            const newBookmark: Bookmark = {
                id: String(Date.now()),
                title: `Page ${readerState.location.page_index + 1} (${Math.round(readerState.progression * 100)}%)`,
                locator,
                created_at_ms: Date.now(),
            };
            const updated = [...bookmarks, newBookmark];
            setBookmarks(updated);
            if (bookmarksStorageKey) {
                localStorage.setItem(bookmarksStorageKey, JSON.stringify(updated));
            }
            setEngineStatus("Bookmark added");
        } catch (error) {
            console.error("Add bookmark failed:", error);
        }
    };

    const jumpToBookmark = (bookmark: Bookmark) => {
        const reader = readerRef.current;
        if (!reader) return;
        try {
            reader.restore_locator_json(JSON.stringify(bookmark.locator));
            reader.render_frame();
            syncReaderState(reader);
            setBookmarksOpen(false);
            setEngineStatus(`Jumped to bookmark: ${bookmark.title}`);
        } catch (error) {
            console.error("Jump to bookmark failed:", error);
        }
    };

    const addHighlightFromSelection = () => {
        const reader = readerRef.current;
        if (!reader || !selection.ranges || selection.ranges.length === 0) return;
        const newHighlights = [...highlights, ...selection.ranges];
        setHighlights(newHighlights);
        if (highlightsStorageKey) {
            localStorage.setItem(highlightsStorageKey, JSON.stringify(newHighlights));
        }
        reader.set_highlights_json(JSON.stringify(newHighlights));
        reader.selection_clear();
        reader.render_frame();
        setSelectedText("");
        setSelection({ text: "", rects: [] });
        setEngineStatus("Highlight created");
    };

    const changeFontSize = (delta: number) => {
        const reader = readerRef.current;
        if (!reader) return;
        const newSize = Math.max(12, Math.min(36, fontSize + delta));
        setFontSize(newSize);
        try {
            reader.set_font_size(newSize);
            reader.render_frame();
            syncReaderState(reader);
        } catch (error) {
            console.error("Change font size failed:", error);
        }
    };

    const changeLineHeight = (val: number) => {
        const reader = readerRef.current;
        if (!reader) return;
        setLineHeight(val);
        try {
            reader.set_line_height(val);
            reader.render_frame();
            syncReaderState(reader);
        } catch (error) {
            console.error("Change line height failed:", error);
        }
    };

    const changeMargin = (level: "compact" | "normal" | "wide") => {
        const reader = readerRef.current;
        if (!reader) return;
        setMarginLevel(level);
        const marginMap = {
            compact: { h: 20, t: 0, b: 16 },
            normal: { h: 44, t: 0, b: 24 },
            wide: { h: 72, t: 10, b: 32 },
        };
        const m = marginMap[level];
        try {
            reader.set_margins(m.h, m.t, m.b);
            reader.render_frame();
            syncReaderState(reader);
        } catch (error) {
            console.error("Change margins failed:", error);
        }
    };

    const changeTheme = (newTheme: "light" | "sepia" | "dark" | "black") => {
        const reader = readerRef.current;
        if (!reader) return;
        setTheme(newTheme);
        const colorMap = {
            light: { fg: [0, 0, 0], bg: [250, 248, 243] },
            sepia: { fg: [91, 70, 54], bg: [244, 236, 216] },
            dark: { fg: [220, 229, 238], bg: [26, 35, 45] },
            black: { fg: [160, 160, 160], bg: [0, 0, 0] },
        };
        const c = colorMap[newTheme];
        try {
            reader.set_colors(c.fg[0], c.fg[1], c.fg[2], c.bg[0], c.bg[1], c.bg[2]);
            reader.render_frame();
            syncReaderState(reader);
        } catch (error) {
            console.error("Change theme failed:", error);
        }
    };

    const changeSpreadMode = (mode: "single" | "double" | "scroll") => {
        const reader = readerRef.current;
        if (!reader) return;
        setSpreadMode(mode);
        try {
            reader.set_spread_mode(mode);
            reader.render_frame();
            syncReaderState(reader);
        } catch (error) {
            console.error("Change spread mode failed:", error);
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
                if (!canvasBounds) return;
                const x = event.clientX - canvasBounds.left;
                const y = event.clientY - canvasBounds.top;
                const result = reader.pointer_up(
                    event.pointerId,
                    x,
                    y,
                    event.timeStamp,
                );
                scheduleTransitionFrame();
                if (claimedPointerRef.current === event.pointerId) {
                    claimedPointerRef.current = undefined;
                    suppressClickRef.current = true;
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
                if ((event.target as HTMLElement).closest("button, nav, input, .search-panel, .bookmarks-panel")) return;
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
            {onToggleZenMode && (
                <button
                    className={`zen-toggle ${zenMode ? "active" : ""}`}
                    aria-label="Toggle Zen mode"
                    onClick={onToggleZenMode}
                    title={zenMode ? "Exit Zen Mode" : "Enter Zen Mode"}
                >
                    {zenMode ? "Exit Zen" : "Zen ⛶"}
                </button>
            )}
            <button
                className={`settings-toggle ${settingsOpen ? "active" : ""}`}
                aria-expanded={settingsOpen}
                aria-controls="reader-settings"
                onClick={() => {
                    setSettingsOpen((open) => !open);
                    setTocOpen(false);
                    setSearchOpen(false);
                    setBookmarksOpen(false);
                }}
            >
                Style ⚙
            </button>
            <button
                className="bookmark-toggle"
                aria-label="Toggle bookmark"
                onClick={() => {
                    setBookmarksOpen((open) => !open);
                    setTocOpen(false);
                    setSearchOpen(false);
                    setSettingsOpen(false);
                }}
            >
                Bookmarks ({bookmarks.length})
            </button>
            <button
                className="search-toggle"
                aria-expanded={searchOpen}
                aria-controls="reader-search"
                onClick={() => {
                    setSearchOpen((open) => !open);
                    setTocOpen(false);
                    setBookmarksOpen(false);
                    setSettingsOpen(false);
                }}
            >
                Search
            </button>
            <button
                className="toc-toggle"
                aria-expanded={tocOpen}
                aria-controls="reader-toc"
                onClick={() => {
                    setTocOpen((open) => !open);
                    setSearchOpen(false);
                    setBookmarksOpen(false);
                    setSettingsOpen(false);
                }}
            >
                Contents
            </button>

            {settingsOpen && (
                <div id="reader-settings" className="settings-panel" aria-label="Reading options">
                    <strong>Reader Settings</strong>
                    
                    <div className="settings-section">
                        <div className="settings-section-title">Theme</div>
                        <div className="theme-options">
                            <button
                                className={`theme-btn theme-light ${theme === "light" ? "active" : ""}`}
                                onClick={() => changeTheme("light")}
                            >
                                Light
                            </button>
                            <button
                                className={`theme-btn theme-sepia ${theme === "sepia" ? "active" : ""}`}
                                onClick={() => changeTheme("sepia")}
                            >
                                Sepia
                            </button>
                            <button
                                className={`theme-btn theme-dark ${theme === "dark" ? "active" : ""}`}
                                onClick={() => changeTheme("dark")}
                            >
                                Dark
                            </button>
                            <button
                                className={`theme-btn theme-black ${theme === "black" ? "active" : ""}`}
                                onClick={() => changeTheme("black")}
                            >
                                Black
                            </button>
                        </div>
                    </div>

                    <div className="settings-section">
                        <div className="settings-section-title">Typography & Size</div>
                        <div className="settings-row">
                            <span>Font Size</span>
                            <div className="settings-btn-group">
                                <button className="settings-btn" onClick={() => changeFontSize(-2)}>A-</button>
                                <span style={{ padding: "0 6px", alignSelf: "center", fontSize: "12px" }}>{fontSize}px</span>
                                <button className="settings-btn" onClick={() => changeFontSize(2)}>A+</button>
                            </div>
                        </div>
                        <div className="settings-row">
                            <span>Line Height</span>
                            <div className="settings-btn-group">
                                <button className={`settings-btn ${lineHeight === 1.2 ? "active" : ""}`} onClick={() => changeLineHeight(1.2)}>1.2</button>
                                <button className={`settings-btn ${lineHeight === 1.5 ? "active" : ""}`} onClick={() => changeLineHeight(1.5)}>1.5</button>
                                <button className={`settings-btn ${lineHeight === 1.8 ? "active" : ""}`} onClick={() => changeLineHeight(1.8)}>1.8</button>
                            </div>
                        </div>
                    </div>

                    <div className="settings-section">
                        <div className="settings-section-title">Layout & Margins</div>
                        <div className="settings-row">
                            <span>Page Spread</span>
                            <div className="settings-btn-group">
                                <button className={`settings-btn ${spreadMode === "single" ? "active" : ""}`} onClick={() => changeSpreadMode("single")}>Single</button>
                                <button className={`settings-btn ${spreadMode === "double" ? "active" : ""}`} onClick={() => changeSpreadMode("double")}>Double</button>
                                <button className={`settings-btn ${spreadMode === "scroll" ? "active" : ""}`} onClick={() => changeSpreadMode("scroll")}>Scroll</button>
                            </div>
                        </div>
                        <div className="settings-row">
                            <span>Margins</span>
                            <div className="settings-btn-group">
                                <button className={`settings-btn ${marginLevel === "compact" ? "active" : ""}`} onClick={() => changeMargin("compact")}>Small</button>
                                <button className={`settings-btn ${marginLevel === "normal" ? "active" : ""}`} onClick={() => changeMargin("normal")}>Medium</button>
                                <button className={`settings-btn ${marginLevel === "wide" ? "active" : ""}`} onClick={() => changeMargin("wide")}>Wide</button>
                            </div>
                        </div>
                    </div>
                </div>
            )}

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

            {searchOpen && (
                <div id="reader-search" className="search-panel" aria-label="Book search">
                    <strong>Search Book</strong>
                    <div className="search-input-wrap">
                        <input
                            type="search"
                            placeholder="Type to search..."
                            value={searchQuery}
                            autoFocus
                            onChange={(e) => handleSearch(e.target.value)}
                        />
                    </div>
                    {searchResults.length === 0 ? (
                        <p>{searchQuery.trim() ? "No matches found" : "Enter a search term"}</p>
                    ) : (
                        searchResults.map((result, idx) => (
                            <button
                                key={idx}
                                className="search-result-item"
                                onClick={() => jumpToSearchResult(result)}
                            >
                                <div className="search-result-title">{result.section_title}</div>
                                <div className="search-result-excerpt">{result.excerpt}</div>
                            </button>
                        ))
                    )}
                </div>
            )}

            {bookmarksOpen && (
                <div className="bookmarks-panel" aria-label="Bookmarks">
                    <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center", padding: "0 12px 6px" }}>
                        <strong>Bookmarks</strong>
                        <button
                            style={{ padding: "4px 8px", background: "#75d6ae", color: "#07110e", border: 0, borderRadius: 4, cursor: "pointer", fontSize: 11, fontWeight: 600 }}
                            onClick={toggleBookmark}
                        >
                            + Add Here
                        </button>
                    </div>
                    {bookmarks.length === 0 ? (
                        <p>No bookmarks saved yet</p>
                    ) : (
                        bookmarks.map((bm) => (
                            <button
                                key={bm.id}
                                className="bookmark-item"
                                onClick={() => jumpToBookmark(bm)}
                            >
                                {bm.title}
                            </button>
                        ))
                    )}
                </div>
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
                        onClick={addHighlightFromSelection}
                        title="Highlight selected text"
                    >
                        Highlight
                    </button>
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
