import { useEffect, useRef } from "react";

/**
 * Animated twinkling starfield background.
 * Renders gold/parchment dots that fade in and out at random rates.
 * Only mounts when the active theme has `particles: true`.
 *
 * Styled via `.hp-particle-canvas` in styles.css (position: fixed, z-index: 0).
 */
export default function ParticleCanvas() {
  const canvasRef = useRef<HTMLCanvasElement>(null);
  const rafRef = useRef<number>(0);

  useEffect(() => {
    const canvas = canvasRef.current;
    if (!canvas) return;

    const ctx = canvas.getContext("2d");
    if (!ctx) return;

    interface Star {
      x: number;
      y: number;
      r: number;
      a: number;
      speed: number;
      color: string;
    }

    let W = 0;
    let H = 0;
    let stars: Star[] = [];

    function init() {
      W = canvas!.width = window.innerWidth;
      H = canvas!.height = window.innerHeight;
      stars = Array.from({ length: 80 }, () => ({
        x: Math.random() * W,
        y: Math.random() * H,
        r: Math.random() * 1.2 + 0.3,
        a: Math.random() * Math.PI * 2,
        speed: Math.random() * 0.003 + 0.001,
        color: Math.random() > 0.7 ? "#c9a84c" : "#f5e6c8",
      }));
    }

    function draw() {
      ctx!.clearRect(0, 0, W, H);
      for (const s of stars) {
        s.a += s.speed;
        ctx!.globalAlpha = (Math.sin(s.a) + 1) / 2 * 0.5 + 0.1;
        ctx!.fillStyle = s.color;
        ctx!.beginPath();
        ctx!.arc(s.x, s.y, s.r, 0, Math.PI * 2);
        ctx!.fill();
      }
      ctx!.globalAlpha = 1;
      rafRef.current = requestAnimationFrame(draw);
    }

    init();
    draw();

    const onResize = () => init();
    window.addEventListener("resize", onResize);

    return () => {
      window.removeEventListener("resize", onResize);
      cancelAnimationFrame(rafRef.current);
    };
  }, []);

  return <canvas ref={canvasRef} className="hp-particle-canvas" />;
}
