import { useState, useEffect, useRef, useCallback } from "react";
import { animate, createScope, createTimeline, spring } from "animejs";
import type { Scope } from "animejs";

/* ------------------------------------------------------------------ */
/*  Road Runner ASCII Art                                              */
/*  Adapted from classic ASCII art — fits sidebar at font-size: 3.8px */
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
/*  Main Component                                                     */
/* ------------------------------------------------------------------ */

export default function LooneyTunesShow() {
  const containerRef = useRef<HTMLDivElement>(null);
  const artRef = useRef<HTMLPreElement>(null);
  const scopeRef = useRef<Scope | null>(null);
  const timelineRef = useRef<ReturnType<typeof createTimeline> | null>(null);
  const meepAudioRef = useRef<HTMLAudioElement | null>(null);
  const [isRunning, setIsRunning] = useState(false);
  const [pokeCount, setPokeCount] = useState(0);
  const [collapsed, setCollapsed] = useState(false);

  // Play meep meep MP3 — audio scoped to component via ref
  const playMeepMeep = useCallback(() => {
    try {
      if (!meepAudioRef.current) {
        meepAudioRef.current = new Audio("/meep-meep.mp3");
        meepAudioRef.current.volume = 0.7;
      }
      meepAudioRef.current.currentTime = 0;
      meepAudioRef.current.play().catch(() => {});
    } catch {
      // Audio not available
    }
  }, []);

  // Idle animation — springy floating bob
  useEffect(() => {
    if (!artRef.current || isRunning || collapsed) return;

    if (scopeRef.current) {
      scopeRef.current.revert();
      scopeRef.current = null;
    }

    const el = artRef.current;

    scopeRef.current = createScope({ root: el }).add(() => {
      animate(el, {
        translateY: [-2, 2],
        duration: 1500,
        ease: spring({ stiffness: 80, damping: 12 }),
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
  }, [isRunning, collapsed]);

  // Handle click — meep meep + smooth run away + spring pop back
  const handleClick = useCallback(() => {
    if (isRunning || !artRef.current) return;

    setPokeCount((c) => c + 1);
    playMeepMeep();

    // Stop idle animation
    if (scopeRef.current) {
      scopeRef.current.revert();
      scopeRef.current = null;
    }

    setIsRunning(true);

    const el = artRef.current;

    // Build timeline: squish → zoom right & shrink into distance → fade → spring back
    const tl = createTimeline({
      onComplete: () => {
        el.style.transform = "";
        el.style.opacity = "1";
        timelineRef.current = null;
        setIsRunning(false);
      },
    });

    timelineRef.current = tl;

    // 1. Squish anticipation (crouching before takeoff)
    tl.add(el, {
      scaleX: [1, 1.15, 0.9],
      scaleY: [1, 0.85, 1.05],
      duration: 250,
      ease: spring({ stiffness: 500, damping: 18 }),
    });

    // 2. Zoom away — swoop right and curve up, shrinking into the distance
    tl.add(
      el,
      {
        translateX: [0, 40, 80, 60, 20],
        translateY: [0, -5, -30, -70, -90],
        scale: [1, 0.8, 0.4, 0.15, 0.1],
        rotate: [0, -5, -15, -10, 0],
        opacity: [1, 1, 0.7, 0.3, 0],
        duration: 1400,
        ease: "inOutQuad",
      },
      250,
    );

    // 3. Hold invisible, reset position for pop-back
    tl.add(
      el,
      {
        opacity: 0,
        scale: 0.1,
        translateX: 0,
        translateY: 0,
        duration: 100,
      },
      1700,
    );

    // 4. Spring pop back at original position (synced to MP3 end ~3.16s)
    tl.add(
      el,
      {
        scale: [0.1, 1],
        opacity: [0, 1],
        translateX: 0,
        translateY: 0,
        rotate: 0,
        duration: 800,
        ease: spring({ stiffness: 200, damping: 15 }),
      },
      3000,
    );
  }, [isRunning, playMeepMeep]);

  // Cleanup on unmount — cancel running timeline + revert scope
  useEffect(() => {
    return () => {
      if (timelineRef.current) {
        timelineRef.current.pause();
        timelineRef.current = null;
      }
      if (scopeRef.current) {
        scopeRef.current.revert();
        scopeRef.current = null;
      }
    };
  }, []);

  const asciiArt = isRunning ? ROAD_RUNNER_RUN : ROAD_RUNNER_IDLE;

  return (
    <div
      ref={containerRef}
      className="sidebar-toon-dancer"
      title="Click for MEEP MEEP!"
    >
      <button
        className="toon-collapse-btn"
        onClick={() => setCollapsed((c) => !c)}
        title={collapsed ? "Show mascot" : "Hide mascot"}
      >
        {collapsed ? "▶ Road Runner" : "▼ Road Runner"}
      </button>
      {!collapsed && (
        <>
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
        </>
      )}
    </div>
  );
}
