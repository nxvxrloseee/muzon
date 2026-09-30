import { Channel } from "@tauri-apps/api/core";
import { useEffect, useRef, useState } from "react";
import { commands } from "../bindings";

/** How fast a bar falls back once its level drops, in full heights a second.
 * Rising is instant: a beat should hit, then fade. */
const FALL_PER_SEC = 1.8;

const STORAGE_KEY = "muzon.visualizer";

/** Whether the visualizer is on - a per-machine display preference, so plain
 * localStorage, and never fatal when storage is unavailable. */
export function useVisualizerEnabled(): [boolean, (on: boolean) => void] {
  const [on, setOn] = useState(() => {
    try {
      return localStorage.getItem(STORAGE_KEY) !== "off";
    } catch {
      return true;
    }
  });
  const set = (next: boolean) => {
    setOn(next);
    try {
      localStorage.setItem(STORAGE_KEY, next ? "on" : "off");
    } catch {
      // Remembering it is a convenience; the toggle works regardless
    }
  };
  return [on, set];
}

/**
 * Spectrum bars filling their container. Subscribes to the backend's frames
 * only while mounted - the analysis posts nothing otherwise.
 */
export function SpectrumBars({ color, className }: { color: string; className?: string }) {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const colorRef = useRef(color);
  colorRef.current = color;

  useEffect(() => {
    const canvas = canvasRef.current;
    const context = canvas?.getContext("2d");
    if (!canvas || !context) return;

    let target: number[] = [];
    let shown: number[] = [];
    let lastFrame = performance.now();
    let drewSilence = false;
    let raf = 0;

    // Heights arrive as bytes, 0..255
    const channel = new Channel<number[]>();
    channel.onmessage = (bars) => {
      target = bars.map((b) => b / 255);
    };
    void commands.subscribeSpectrum(channel);

    const resize = () => {
      const ratio = window.devicePixelRatio || 1;
      canvas.width = Math.round(canvas.clientWidth * ratio);
      canvas.height = Math.round(canvas.clientHeight * ratio);
      drewSilence = false;
    };
    const observer = new ResizeObserver(resize);
    observer.observe(canvas);
    resize();

    const draw = (now: number) => {
      raf = requestAnimationFrame(draw);
      const dt = Math.min((now - lastFrame) / 1000, 0.1);
      lastFrame = now;

      if (shown.length !== target.length) shown = target.map(() => 0);
      let silent = true;
      for (let i = 0; i < shown.length; i++) {
        shown[i] = Math.max(target[i], shown[i] - FALL_PER_SEC * dt);
        if (shown[i] > 0.001) silent = false;
      }
      // Paused or stopped: one blank frame, then nothing to repaint
      if (silent && drewSilence) return;
      drewSilence = silent;

      const { width, height } = canvas;
      context.clearRect(0, 0, width, height);
      if (shown.length === 0) return;
      const slot = width / shown.length;
      const gap = Math.max(1, slot * 0.25);
      const radius = Math.min((slot - gap) / 2, 6);
      // Plain #rrggbb, whatever alpha the theme colour carried, so the
      // gradient's own alpha can be appended
      const base = colorRef.current.trim().slice(0, 7);
      const gradient = context.createLinearGradient(0, height, 0, 0);
      gradient.addColorStop(0, `${base}cc`);
      gradient.addColorStop(1, `${base}22`);
      context.fillStyle = gradient;
      for (let i = 0; i < shown.length; i++) {
        const h = shown[i] * height;
        if (h < 1) continue;
        context.beginPath();
        context.roundRect(i * slot + gap / 2, height - h, slot - gap, h, [radius, radius, 0, 0]);
        context.fill();
      }
    };
    raf = requestAnimationFrame(draw);

    return () => {
      cancelAnimationFrame(raf);
      observer.disconnect();
      void commands.unsubscribeSpectrum();
    };
  }, []);

  return <canvas ref={canvasRef} className={className} />;
}
