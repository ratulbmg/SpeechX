import { useEffect, useRef } from "react";

interface Props {
  /** 0..1 smoothed microphone level, from the audio-level pipeline */
  level: number;
  /** false → freeze and stop the rAF loop */
  active: boolean;
}

const prefersReducedMotion = () =>
  window.matchMedia?.("(prefers-reduced-motion: reduce)").matches ?? false;

const SonicWaveform: React.FC<Props> = ({ level, active }) => {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const levelRef = useRef(0);
  const rafRef = useRef<number | undefined>(undefined);

  // Keep the latest level in a ref so the rAF closure isn't re-created
  // on every render.
  useEffect(() => {
    levelRef.current = level;
  }, [level]);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;
    const ctx = canvas.getContext("2d", { alpha: false });
    if (!ctx) return;

    const reducedMotion = prefersReducedMotion();

    let time = 0;
    let smoothed = 0;

    const resize = () => {
      const dpr = window.devicePixelRatio || 1;
      canvas.width = canvas.clientWidth * dpr;
      canvas.height = canvas.clientHeight * dpr;
      ctx.setTransform(dpr, 0, 0, dpr, 0, 0); // retina scaling
    };

    const drawReducedMotion = () => {
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      const mid = h / 2;

      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, w, h);

      const pulse = 0.35 + Math.sin(time) * 0.15 + levelRef.current * 0.3;
      ctx.strokeStyle = `rgba(0, 255, 192, ${Math.min(1, pulse)})`;
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(0, mid);
      ctx.lineTo(w, mid);
      ctx.stroke();

      time += 0.03;
      rafRef.current = requestAnimationFrame(drawReducedMotion);
    };

    const draw = () => {
      const w = canvas.clientWidth;
      const h = canvas.clientHeight;
      const mid = h / 2;

      // Asymmetric smoothing: snap up fast on speech onset, decay
      // slowly. Symmetric smoothing looks sluggish and dead.
      const target = levelRef.current;
      const k = target > smoothed ? 0.5 : 0.12;
      smoothed += (target - smoothed) * k;

      // Floor at 0.05 so the line still breathes in silence rather
      // than going flat and looking broken.
      const amp = Math.max(0.05, smoothed);

      const noiseAmp = h * (0.05 + amp * 0.2);
      const spikeAmp = h * (0.08 + amp * 0.55);

      // Trailing motion blur — requires an opaque canvas.
      ctx.fillStyle = "rgba(0, 0, 0, 0.1)";
      ctx.fillRect(0, 0, w, h);

      const lineCount = 14; // was 60 in the ambient full-screen original
      const segmentCount = 60;

      for (let i = 0; i < lineCount; i++) {
        ctx.beginPath();
        const progress = i / lineCount;
        const intensity = Math.sin(progress * Math.PI);

        // Brightness also tracks amplitude, so louder reads as hotter.
        ctx.strokeStyle = `rgba(0, 255, 192, ${intensity * (0.25 + amp * 0.45)})`;
        ctx.lineWidth = 1.2;

        for (let j = 0; j <= segmentCount; j++) {
          const x = (j / segmentCount) * w;
          const noise = Math.sin(j * 0.1 + time + i * 0.2) * noiseAmp;
          const spike = Math.cos(j * 0.2 + time + i * 0.1) * Math.sin(j * 0.05 + time) * spikeAmp;
          const y = mid + noise + spike;
          j === 0 ? ctx.moveTo(x, y) : ctx.lineTo(x, y);
        }
        ctx.stroke();
      }

      // Speed up slightly when speaking — subtle but it reads as alive.
      time += 0.02 + amp * 0.02;
      rafRef.current = requestAnimationFrame(draw);
    };

    window.addEventListener("resize", resize);
    resize();

    if (active) {
      (reducedMotion ? drawReducedMotion : draw)();
    } else {
      // Paint one still frame so the canvas isn't left blank/stale.
      ctx.fillStyle = "#000";
      ctx.fillRect(0, 0, canvas.clientWidth, canvas.clientHeight);
    }

    return () => {
      if (rafRef.current) cancelAnimationFrame(rafRef.current);
      window.removeEventListener("resize", resize);
    };
  }, [active]);

  return <canvas ref={canvasRef} className="waveform" />;
};

export default SonicWaveform;
