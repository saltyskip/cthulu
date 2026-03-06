import { useState, useEffect, useRef, useCallback } from "react";
import { animate, createScope, spring } from "animejs";
import type { Scope } from "animejs";

/* ------------------------------------------------------------------ */
/*  Road Runner ASCII Art                                              */
/*  Adapted from classic ASCII art — fits sidebar at font-size: 5px   */
/* ------------------------------------------------------------------ */

const ROAD_RUNNER_IDLE = `\
                            /  \\      __
.---.                  _   /   /   _.~  \\
\\    \`.               / \\ /   /.-~    __/
 \`\\    \\              |  |   |/    .-~ __
   \\    \\             |  |   |   .'--~~  \\
    \\    \\            |  |   \`  ' _______/
     \\    \\           |  \`        /
 .--. \\    \\          |    \`     /
 \\   \`.\\    \\          \\        /
  \`\\   \\     \\          \`\\     (
    \\   \\     \\           > ,-.-.
     \\   \`.    \\         /  |  \\ \\
      \\    .    \\       /___| O |O\\     ,
   .-. \\    ;    |    /\`    \`^-.\\.\\-'\`--'/
   \\  \`;         |   |                 /
    \`\\  \\        |   \`.     \`--..____,'
      \\  \`.      |     \`._     _.-'^
       \\   .     /         \`|\`|\`
     .-.\\.       /           | |
     \\  \`\\     /            | |
      \`\\  \`   |             | |
        \\     |             | |
       .-.    |             | |
       \\  \`.   \\            | |
        \`\\      \\           | |
          \\      \\          | |
           \\_____ :-'~~~~~'-' ;
           /____;\\---.         :
          <____(     \`.       ;
            \\___\\     ;     .'
               /\`\`--'~___.-'
              /\\___/^/__/
             /   /' /\`/'
             \\  \\   \`\\ \\
              \`\\ \\    \\ \\
                \\ \\    \\ \\
                 \\ \\    \\ \\
                  \\ \\    \\ \\     ______
                   \\ \\ ___\\ \\'~\`\`______)>
                    \\ \\___ _______ __)>
                _____\\ \\'~\`\`______)>
              <(_______.._______)>`;

const ROAD_RUNNER_RUN = `\
                                          .
                            /  \\      __ /
.---.                  _   /   /   _.~ /
\\    \`.               / \\ /   /.-~   /
 \`\\    \\              |  |   |/   .-~
   \\    \\             |  |   |  .'--~~\\
    \\    \\            |  |   \` ' _____/
     \\    \\           |  \`       /
 .--. \\    \\          |   \`     /
 \\   \`.\\    \\          \\       /
  \`\\   \\     \\          \`\\    (
    \\   \\     \\           > ,-.-. =
     \\   \`.    \\         /  |  \\ \\==
      \\    .    \\       /___| O |O\\===,
   .-. \\    ;    |    /\`    \`^-.\\.\\-'\\==/
   \\  \`;         |   |               /
    \`\\  \\        |   \`.    \`--..___,'
      \\  \`.      |     \`._    _.-'^
       \\   .     /         \`|\`|\`
     .-.\\.       /           | |
     \\  \`\\     /            | |
      \`\\  \`   |             | |
        \\     |             | |
       .-.    |             | |
       \\  \`.   \\            | |
        \`\\      \\           | |
          \\      \\          | |
           \\_____ :-'~~~~~'-' ;
           /____;\\---.         :
          <____(     \`.       ;
            \\___\\     ;     .'          =
               /\`\`--'~___.-'          ==
              /\\___/^/__/           ====
             /   /' /\`/' =========/
             \\  \\   \`\\ \\
              \`\\ \\    \\ \\
                \\ \\    \\ \\    ~~~~~~
                 \\ \\    \\ \\  ~~~~~~
                  \\ \\    \\ \\    ______
                   \\ \\ ___\\ \\'~\`\`______)>
                    \\ \\___ _______ __)>
                _____\\ \\'~\`\`______)>
              <(_______.._______)>`;

/* ------------------------------------------------------------------ */
/*  "Meep Meep" sound via Web Audio API                                */
/* ------------------------------------------------------------------ */

let audioCtx: AudioContext | null = null;

function playMeepMeep() {
  try {
    if (!audioCtx) audioCtx = new AudioContext();
    const ctx = audioCtx;

    const playBeep = (startTime: number, freq: number, dur: number) => {
      const osc = ctx.createOscillator();
      const gain = ctx.createGain();
      osc.connect(gain);
      gain.connect(ctx.destination);
      osc.type = "sine";
      osc.frequency.setValueAtTime(freq, startTime);
      // Quick pitch slide up for cartoon feel
      osc.frequency.linearRampToValueAtTime(freq * 1.3, startTime + dur * 0.3);
      osc.frequency.linearRampToValueAtTime(freq * 1.1, startTime + dur);
      gain.gain.setValueAtTime(0, startTime);
      gain.gain.linearRampToValueAtTime(0.15, startTime + 0.01);
      gain.gain.setValueAtTime(0.15, startTime + dur - 0.02);
      gain.gain.linearRampToValueAtTime(0, startTime + dur);
      osc.start(startTime);
      osc.stop(startTime + dur);
    };

    const now = ctx.currentTime;
    // Two quick high-pitched beeps: "meep meep!"
    playBeep(now, 1800, 0.12);
    playBeep(now + 0.18, 2200, 0.12);
  } catch {
    // Audio not available — no-op
  }
}

/* ------------------------------------------------------------------ */
/*  Main Component                                                     */
/* ------------------------------------------------------------------ */

export default function LooneyTunesShow() {
  const containerRef = useRef<HTMLDivElement>(null);
  const artRef = useRef<HTMLPreElement>(null);
  const scopeRef = useRef<Scope | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [pokeCount, setPokeCount] = useState(0);
  const runTimeoutRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  // Idle animation — gentle breathing/floating
  useEffect(() => {
    if (!artRef.current || isRunning) return;

    if (scopeRef.current) {
      scopeRef.current.revert();
      scopeRef.current = null;
    }

    const el = artRef.current;

    scopeRef.current = createScope({ root: el }).add(() => {
      animate(el, {
        translateY: [-1, 1],
        duration: 800,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
    });

    return () => {
      if (scopeRef.current) {
        scopeRef.current.revert();
        scopeRef.current = null;
      }
    };
  }, [isRunning]);

  // Handle click — meep meep + run away + come back
  const handleClick = useCallback(() => {
    if (isRunning) return;

    setPokeCount((c) => c + 1);

    // Play sound
    playMeepMeep();

    // Stop idle animation
    if (scopeRef.current) {
      scopeRef.current.revert();
      scopeRef.current = null;
    }

    setIsRunning(true);

    // Run away animation
    if (artRef.current) {
      const el = artRef.current;

      // Quick squish then zoom off to the right
      animate(el, {
        scaleX: [1, 1.2, 0.8],
        scaleY: [1, 0.8, 1.1],
        duration: 200,
        ease: spring({ stiffness: 400, damping: 15 }),
      });

      // After squish, zoom off screen
      setTimeout(() => {
        if (!artRef.current) return;
        animate(artRef.current, {
          translateX: [0, 400],
          opacity: [1, 0],
          duration: 300,
          ease: "inExpo",
        });
      }, 200);
    }

    // Clear any existing return timeout
    if (runTimeoutRef.current) clearTimeout(runTimeoutRef.current);

    // Come back after a beat
    runTimeoutRef.current = setTimeout(() => {
      if (!artRef.current) return;
      // Slide back in from left
      animate(artRef.current, {
        translateX: [-300, 0],
        opacity: [0, 1],
        duration: 500,
        ease: "outExpo",
        onComplete: () => {
          setIsRunning(false);
        },
      });
    }, 1200);
  }, [isRunning]);

  // Cleanup timeout on unmount
  useEffect(() => {
    return () => {
      if (runTimeoutRef.current) clearTimeout(runTimeoutRef.current);
    };
  }, []);

  const asciiArt = isRunning ? ROAD_RUNNER_RUN : ROAD_RUNNER_IDLE;

  return (
    <div
      ref={containerRef}
      className="sidebar-toon-dancer"
      title="Click for MEEP MEEP!"
    >
      <div className="toon-ascii-container" onClick={handleClick}>
        <pre ref={artRef} className="toon-ascii toon-ascii-roadrunner">
          {asciiArt}
        </pre>
      </div>
      <div className="toon-char-name">Road Runner</div>
      <div className="toon-dialog">
        {isRunning ? "MEEP MEEP! *zooooom*" : "Click me... if you can!"}
      </div>
      {pokeCount > 0 && (
        <div className="toon-poke-counter">Meeps: {pokeCount}</div>
      )}
    </div>
  );
}
