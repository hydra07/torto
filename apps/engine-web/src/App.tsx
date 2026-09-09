import { useState } from "react";
import { ReaderCanvas } from "./ReaderCanvas";

export function App() {
    const [bookName, setBookName] = useState("No book loaded");
    const [bookBytes, setBookBytes] = useState<Uint8Array | undefined>();
    return (
        <main className="app">
            <header>
                <div>
                    <span className="eyebrow">TORTO ENGINE LAB</span>
                    <h1>Reader engine web harness</h1>
                </div>
                <label className="file">
                    <input
                        type="file"
                        accept=".epub,application/epub+zip"
                        onChange={async (e) => {
                            const file = e.target.files?.[0];
                            if (file) {
                                setBookName(file.name);
                                setBookBytes(
                                    new Uint8Array(await file.arrayBuffer()),
                                );
                            }
                        }}
                    />
                    Open book
                </label>
            </header>
            <section className="reader">
                <ReaderCanvas bytes={bookBytes} fileName={bookName} />
                <div className="hud">
                    <span>{bookName}</span>
                    <span>Rust engine / WASM / WebGPU</span>
                </div>
            </section>
            <footer>
                Browser owns the surface and GPU. The Rust engine owns parsing,
                layout, pagination, navigation and reading position.
            </footer>
        </main>
    );
}
