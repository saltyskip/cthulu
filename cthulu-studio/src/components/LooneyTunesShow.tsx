import { useState, useEffect, useRef, useCallback } from "react";
import { animate, createScope, spring, stagger } from "animejs";
import type { Scope } from "animejs";

/* ------------------------------------------------------------------ */
/*  ACTS — 32 random skits featuring Bugs, Daffy, Elmer & Road Runner */
/* ------------------------------------------------------------------ */

type Character = "bugs" | "daffy" | "elmer" | "roadrunner";

interface Act {
  character: Character;
  line: string;
  /** animation preset key */
  anim: string;
}

const ACTS: Act[] = [
  // ---- BUGS BUNNY (8) ----
  { character: "bugs", line: "Ehhh... What's up, doc?", anim: "chomp" },
  { character: "bugs", line: "Ain't I a stinker?", anim: "smug" },
  { character: "bugs", line: "Of course you realize... this means war!", anim: "point" },
  { character: "bugs", line: "Watch me paste this one!", anim: "wave" },
  { character: "bugs", line: "I knew I shoulda taken that left turn at Albuquerque!", anim: "shrug" },
  { character: "bugs", line: "What a maroon!", anim: "laugh" },
  { character: "bugs", line: "Nyaaah... what's cookin', doc?", anim: "chomp" },
  { character: "bugs", line: "Don't take life too seriously — you'll never get out alive!", anim: "dance" },
  // ---- DAFFY DUCK (8) ----
  { character: "daffy", line: "You're dethpicable!", anim: "rage" },
  { character: "daffy", line: "It'th mine! All mine!", anim: "grab" },
  { character: "daffy", line: "Duck theason! Fire!", anim: "dodge" },
  { character: "daffy", line: "I'm not crazy — I just don't give a darn!", anim: "strut" },
  { character: "daffy", line: "Woo-hoo! Woo-hoo! Woo-hoo!", anim: "bounce" },
  { character: "daffy", line: "Thufferin' thuccotash... wait that's the other guy.", anim: "confused" },
  { character: "daffy", line: "I demand that you shoot me now!", anim: "rage" },
  { character: "daffy", line: "Pronoun trouble — it's RABBIT season!", anim: "point" },
  // ---- ELMER FUDD (8) ----
  { character: "elmer", line: "Shhh... be vewy vewy quiet. I'm hunting wabbits!", anim: "sneak" },
  { character: "elmer", line: "Hehehehehehe!", anim: "laugh" },
  { character: "elmer", line: "Oh you wascawwy wabbit!", anim: "aim" },
  { character: "elmer", line: "Come hewe, you pesky wabbit!", anim: "chase" },
  { character: "elmer", line: "Wabbit twacks!", anim: "sneak" },
  { character: "elmer", line: "I'll bwast that wabbit to smitheweens!", anim: "aim" },
  { character: "elmer", line: "Kill the wabbit! Kill the wabbit!", anim: "chase" },
  { character: "elmer", line: "That doggone wabbit twickd me again!", anim: "confused" },
  // ---- ROAD RUNNER (8) ----
  { character: "roadrunner", line: "MEEP MEEP!", anim: "zoom" },
  { character: "roadrunner", line: "BEEP BEEP! *whoooosh*", anim: "zoom" },
  { character: "roadrunner", line: "Meep meep! *tongue out* THBBBT!", anim: "taunt" },
  { character: "roadrunner", line: "*zooooom* ...MEEP MEEP!", anim: "zoom" },
  { character: "roadrunner", line: "BEEP BEEP! *dust cloud*", anim: "dust" },
  { character: "roadrunner", line: "*skreeeeech* MEEP! *zooom*", anim: "skid" },
  { character: "roadrunner", line: "Meep? ...MEEP MEEP! *vanish*", anim: "peek" },
  { character: "roadrunner", line: "*SONIC BOOM* BEEEEP BEEEEEP!", anim: "zoom" },
];

/* ------------------------------------------------------------------ */
/*  POKE REACTIONS — character-specific lines when clicked             */
/* ------------------------------------------------------------------ */

const POKE_REACTIONS: Record<Character, string[]> = {
  bugs: [
    "Hey! Watch the fur, doc!",
    "Do ya mind? I'm eatin' here!",
    "Cut it out! ...Ain't I a stinker?",
    "Okay pal, you asked for it!",
    "Pokin' a rabbit? Really? REALLY?",
    "One more time and it's WAR, doc!",
    "What are ya, some kinda wise guy?",
    "That tickles! Do it again!",
  ],
  daffy: [
    "Don't touch me! I'm a THTAR!",
    "Hey! Hands off the feathers!",
    "Ow! That's MY personal thpace!",
    "Thtop poking me, you imbecile!",
    "I'll thue! I'll thue the whole lot of ya!",
    "You have no idea who you're dealing with!",
    "Mother!!! Someone's touching me!",
    "Do that again and I'll... I'll... ow!",
  ],
  elmer: [
    "Oww! Don't poke the huntew!",
    "Hey! I'm twying to concentwate!",
    "Shhh! You'll scare the wabbits!",
    "Stop dat wight now!",
    "Hehehehe... dat tickews!",
    "I'll get you too, you pesky poker!",
    "Wook what you made me do!",
    "Be vewy caweful where you poke!",
  ],
  roadrunner: [
    "MEEP! *indignant stare*",
    "*dodges poke* MEEP MEEP!",
    "BEEP! *pecks your finger*",
    "*already 3 miles away* meep.",
    "MEEP?! *ruffled feathers*",
    "*stops* ...MEEP! *zooms off*",
    "BEEP BEEP! *too fast to poke*",
    "*vibrates angrily* MEEEEEP!",
  ],
};

/** After this many pokes, unlock special easter-egg lines */
const POKE_EASTER_EGGS: { threshold: number; character: Character; line: string }[] = [
  { threshold: 5, character: "bugs", line: "You've poked me 5 times... What IS your deal, doc?" },
  { threshold: 10, character: "daffy", line: "TEN POKETH?! This is harassment! I'm calling my lawyer!" },
  { threshold: 15, character: "elmer", line: "15 pokes?! Even the wabbit doesn't bug me this much!" },
  { threshold: 20, character: "roadrunner", line: "20 pokes?! MEEP MEEP MEEP MEEP MEEP!" },
  { threshold: 25, character: "bugs", line: "25 pokes. Okay I respect the commitment. Wanna carrot?" },
  { threshold: 30, character: "daffy", line: "THIRTY?! You need a hobby. Theriouthly." },
  { threshold: 42, character: "elmer", line: "42 pokes! That's the meaning of wife... I mean LIFE!" },
  { threshold: 50, character: "roadrunner", line: "50 pokes! *breaks sound barrier* MEEEEEEEEP!" },
  { threshold: 69, character: "bugs", line: "69 pokes, doc. Nice. ...Ain't I a stinker?" },
];

const CHAR_NAMES: Record<Character, string> = {
  bugs: "Bugs Bunny",
  daffy: "Daffy Duck",
  elmer: "Elmer Fudd",
  roadrunner: "Road Runner",
};

/* ------------------------------------------------------------------ */
/*  ASCII Art Characters                                              */
/*  Each has poses mapped from animation presets                       */
/*  Lines wrapped in <span> with color classes for anime.js targeting  */
/* ------------------------------------------------------------------ */

type Pose = "idle" | "talk" | "arms-up" | "point" | "duck" | "sneak" | "zoom" | "taunt" | "skid" | "peek" | "dust";

interface AsciiFrame {
  lines: string[];
  /** CSS class names per line for coloring */
  colors: string[];
}

/* -- BUGS BUNNY -- */
const BUGS_FRAMES: Record<string, AsciiFrame> = {
  idle: {
    lines: [
      "  (\\(\\        ",
      "  ( -.-)      ",
      "  o_(\")(\")    ",
    ],
    colors: ["ascii-ear", "ascii-face", "ascii-body"],
  },
  talk: {
    lines: [
      "  (\\(\\        ",
      "  ( ^o^)      ",
      "  o_(\")(\")    ",
    ],
    colors: ["ascii-ear", "ascii-face", "ascii-body"],
  },
  "arms-up": {
    lines: [
      "  (\\(\\        ",
      " \\( -.-)/ ~c  ",
      "  (\")(\")      ",
    ],
    colors: ["ascii-ear", "ascii-face", "ascii-body"],
  },
  point: {
    lines: [
      "  (\\(\\        ",
      "  ( -.-)  ~c  ",
      "  (\")(\") >    ",
    ],
    colors: ["ascii-ear", "ascii-face", "ascii-body"],
  },
  duck: {
    lines: [
      "              ",
      "  (\\(\\        ",
      "  ( x.x)      ",
      "  _(\")(\")_    ",
    ],
    colors: ["ascii-ear", "ascii-ear", "ascii-face", "ascii-body"],
  },
  sneak: {
    lines: [
      "   (\\(\\       ",
      "   ( -.-) ~c  ",
      "  o_(\")(\")    ",
    ],
    colors: ["ascii-ear", "ascii-face", "ascii-body"],
  },
};

/* -- DAFFY DUCK -- */
const DAFFY_FRAMES: Record<string, AsciiFrame> = {
  idle: {
    lines: [
      "   ~\\~       ",
      "   (>.<)     ",
      "  _/|  |\\_   ",
      "    d  b     ",
    ],
    colors: ["ascii-tuft", "ascii-face", "ascii-body", "ascii-feet"],
  },
  talk: {
    lines: [
      "   ~\\~       ",
      "   (>O<)     ",
      "  _/|  |\\_   ",
      "    d  b     ",
    ],
    colors: ["ascii-tuft", "ascii-face", "ascii-body", "ascii-feet"],
  },
  "arms-up": {
    lines: [
      "   ~\\~       ",
      "  \\(>.<)/    ",
      "   |  |      ",
      "    d  b     ",
    ],
    colors: ["ascii-tuft", "ascii-face", "ascii-body", "ascii-feet"],
  },
  point: {
    lines: [
      "   ~\\~       ",
      "   (>.<)--   ",
      "   /|  |     ",
      "    d  b     ",
    ],
    colors: ["ascii-tuft", "ascii-face", "ascii-body", "ascii-feet"],
  },
  duck: {
    lines: [
      "             ",
      "   ~\\~       ",
      "   (>.<)     ",
      "  _/dllb\\_   ",
    ],
    colors: ["ascii-tuft", "ascii-tuft", "ascii-face", "ascii-body"],
  },
  sneak: {
    lines: [
      "    ~\\~      ",
      "    (>.<) >  ",
      "   _/| |     ",
      "     d b     ",
    ],
    colors: ["ascii-tuft", "ascii-face", "ascii-body", "ascii-feet"],
  },
};

/* -- ELMER FUDD -- */
const ELMER_FRAMES: Record<string, AsciiFrame> = {
  idle: {
    lines: [
      "   ___       ",
      "  |o_o|      ",
      "  /| |\\=~   ",
      "   d  b      ",
    ],
    colors: ["ascii-hat", "ascii-face", "ascii-body", "ascii-feet"],
  },
  talk: {
    lines: [
      "   ___       ",
      "  |o_O|      ",
      "  /| |\\=~   ",
      "   d  b      ",
    ],
    colors: ["ascii-hat", "ascii-face", "ascii-body", "ascii-feet"],
  },
  "arms-up": {
    lines: [
      "   ___       ",
      "  |o_o|      ",
      " \\|| ||/=~  ",
      "   d  b      ",
    ],
    colors: ["ascii-hat", "ascii-face", "ascii-body", "ascii-feet"],
  },
  point: {
    lines: [
      "   ___       ",
      "  |o_o| =~  ",
      "  /| |/      ",
      "   d  b      ",
    ],
    colors: ["ascii-hat", "ascii-face", "ascii-body", "ascii-feet"],
  },
  duck: {
    lines: [
      "             ",
      "   ___       ",
      "  |o_o|=~   ",
      "  /d  b\\    ",
    ],
    colors: ["ascii-hat", "ascii-hat", "ascii-face", "ascii-body"],
  },
  sneak: {
    lines: [
      "    ___      ",
      "   |o_o|     ",
      "  /| |\\=~   ",
      "    d  b     ",
    ],
    colors: ["ascii-hat", "ascii-face", "ascii-body", "ascii-feet"],
  },
};

/* -- ROAD RUNNER -- compact sidebar version inspired by the classic */
const RUNNER_FRAMES: Record<string, AsciiFrame> = {
  idle: {
    lines: [
      "     .---,    ",
      "    /  o  \\>  ",
      "   |  ___/    ",
      "   \\_/| |     ",
      "     _| |_    ",
    ],
    colors: ["ascii-crest", "ascii-rr-face", "ascii-rr-beak", "ascii-rr-body", "ascii-rr-legs"],
  },
  zoom: {
    lines: [
      " ===.---,     ",
      " ==/  o  \\>   ",
      " ==| ___/     ",
      "   \\_/| |     ",
      "   =_| |_=    ",
    ],
    colors: ["ascii-rr-dust", "ascii-rr-dust", "ascii-rr-beak", "ascii-rr-body", "ascii-rr-legs"],
  },
  taunt: {
    lines: [
      "     .---,    ",
      "    /  ^  \\>P ",
      "   |  ___/    ",
      "   \\_/| |     ",
      "     _| |_    ",
    ],
    colors: ["ascii-crest", "ascii-rr-face", "ascii-rr-beak", "ascii-rr-body", "ascii-rr-legs"],
  },
  dust: {
    lines: [
      "  *  .---,    ",
      " * */  o  \\>  ",
      "  * | ___/    ",
      " *  \\_/| | *  ",
      "  *  _| |_ *  ",
    ],
    colors: ["ascii-rr-dust", "ascii-rr-dust", "ascii-rr-beak", "ascii-rr-body", "ascii-rr-legs"],
  },
  skid: {
    lines: [
      "     .---,    ",
      "    /  O  \\>  ",
      "   |  ___/    ",
      "  ~\\_/| |~    ",
      "  ~~_| |_~~   ",
    ],
    colors: ["ascii-crest", "ascii-rr-face", "ascii-rr-beak", "ascii-rr-body", "ascii-rr-dust"],
  },
  peek: {
    lines: [
      "        ,     ",
      "       / o>   ",
      "      |_/     ",
      "              ",
      "              ",
    ],
    colors: ["ascii-crest", "ascii-rr-face", "ascii-rr-beak", "ascii-rr-body", "ascii-rr-body"],
  },
};

const CHAR_FRAMES: Record<Character, Record<string, AsciiFrame>> = {
  bugs: BUGS_FRAMES,
  daffy: DAFFY_FRAMES,
  elmer: ELMER_FRAMES,
  roadrunner: RUNNER_FRAMES,
};

/** Map animation preset -> ASCII pose key */
function animToPose(character: Character, anim: string): string {
  // Road Runner has its own pose names
  if (character === "roadrunner") {
    if (["zoom"].includes(anim)) return "zoom";
    if (["taunt"].includes(anim)) return "taunt";
    if (["dust"].includes(anim)) return "dust";
    if (["skid"].includes(anim)) return "skid";
    if (["peek"].includes(anim)) return "peek";
    return "idle";
  }
  // Original 3 characters
  switch (anim) {
    case "chomp": case "laugh": return "talk";
    case "wave": case "shrug": case "dance": case "bounce":
    case "rage": case "grab": case "chase": return "arms-up";
    case "point": case "strut": case "aim": return "point";
    case "dodge": return "duck";
    case "sneak": return "sneak";
    default: return "idle";
  }
}

/* ------------------------------------------------------------------ */
/*  Animation engine (anime.js v4 on ASCII spans)                     */
/* ------------------------------------------------------------------ */

function applyAnimations(container: HTMLElement, anim: string): void {
  const lines = container.querySelectorAll<HTMLElement>(".ascii-line");
  const allChars = container.querySelectorAll<HTMLElement>(".ascii-char");

  // Base idle bounce — entire ASCII block floats up and down
  animate(container, {
    translateY: [-1, 1],
    duration: 600 + Math.random() * 200,
    ease: "inOutSine",
    alternate: true,
    loop: true,
  });

  switch (anim) {
    /* -- Sound/talk animations -- */
    case "chomp":
    case "laugh":
      if (lines[1]) {
        animate(lines[1], {
          scaleX: [1, 1.04, 0.96, 1],
          duration: 350,
          ease: "inOutSine",
          loop: true,
        });
      }
      break;

    case "wave":
    case "point":
      animate(container, {
        rotate: [-1.5, 1.5],
        duration: 800,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "smug":
    case "strut":
      animate(container, {
        translateX: [-2, 2],
        rotate: [-1, 1],
        duration: 1000,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "shrug":
    case "confused":
      animate(container, {
        rotate: [-3, 3],
        duration: 600,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "dance":
    case "bounce":
      animate(container, {
        translateY: [-4, 4],
        duration: 300,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      if (allChars.length > 0) {
        animate(allChars, {
          opacity: [1, 0.6, 1],
          duration: 300,
          ease: "inOutSine",
          delay: stagger(10),
          loop: true,
        });
      }
      break;

    case "rage":
      animate(container, {
        translateX: [-3, 3],
        duration: 80,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "grab":
      animate(container, {
        scaleX: [1, 1.06, 1],
        translateX: [0, 3, 0],
        duration: 600,
        ease: "inOutSine",
        loop: true,
      });
      break;

    case "dodge":
      animate(container, {
        translateX: [-6, 6],
        rotate: [-2, 2],
        duration: 500,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "sneak":
      animate(container, {
        translateX: [-3, 3],
        translateY: [0, 2, 0],
        duration: 1200,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "aim":
      animate(container, {
        translateX: [-0.5, 0.5],
        translateY: [-0.5, 0.5],
        duration: 150,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "chase":
      animate(container, {
        translateX: [-4, 4],
        translateY: [-2, 2],
        duration: 250,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    /* -- Road Runner specific animations -- */
    case "zoom":
      // Speed blur — fast horizontal oscillation + character streaks
      animate(container, {
        translateX: [-8, 8],
        duration: 120,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      if (allChars.length > 0) {
        animate(allChars, {
          opacity: [1, 0.3, 1],
          duration: 120,
          ease: "linear",
          delay: stagger(5),
          loop: true,
        });
      }
      break;

    case "taunt":
      // Cocky head bob
      animate(container, {
        translateY: [-3, 3],
        rotate: [-2, 2],
        duration: 400,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;

    case "dust":
      // Dust cloud shake
      animate(container, {
        translateX: [-2, 2],
        translateY: [-1, 1],
        duration: 150,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      if (allChars.length > 0) {
        animate(allChars, {
          opacity: [0.5, 1, 0.5],
          duration: 400,
          ease: "inOutSine",
          delay: stagger(8),
          loop: true,
        });
      }
      break;

    case "skid":
      // Screech to a halt — fast decel
      animate(container, {
        translateX: [10, -1, 0],
        duration: 800,
        ease: "outExpo",
        loop: true,
        loopDelay: 400,
      });
      break;

    case "peek":
      // Peek in from side
      animate(container, {
        translateX: [20, 0, 0, 20],
        opacity: [0, 1, 1, 0],
        duration: 2000,
        ease: "inOutSine",
        loop: true,
      });
      break;

    default:
      animate(container, {
        rotate: [-0.5, 0.5],
        duration: 1500,
        ease: "inOutSine",
        alternate: true,
        loop: true,
      });
      break;
  }
}

/** One-shot "poked!" squish animation */
function playPokeReaction(container: HTMLElement): void {
  animate(container, {
    scaleX: [1, 1.15, 0.9, 1.05, 1],
    scaleY: [1, 0.85, 1.1, 0.95, 1],
    duration: 400,
    ease: spring({ stiffness: 300, damping: 12 }),
  });
}

/* ------------------------------------------------------------------ */
/*  ASCII Art Renderer                                                 */
/* ------------------------------------------------------------------ */

function AsciiCharacter({ character, anim }: { character: Character; anim: string }) {
  const pose = animToPose(character, anim);
  const frames = CHAR_FRAMES[character];
  const frame = frames[pose] || frames["idle"];

  return (
    <pre className={`toon-ascii toon-ascii-${character}`}>
      {frame.lines.map((line, i) => (
        <span key={i} className={`ascii-line ${frame.colors[i]}`}>
          {line.split("").map((ch, j) => (
            <span key={j} className="ascii-char">{ch}</span>
          ))}
          {"\n"}
        </span>
      ))}
    </pre>
  );
}

/* ------------------------------------------------------------------ */
/*  Helpers                                                            */
/* ------------------------------------------------------------------ */

function shuffleArray<T>(arr: T[]): T[] {
  const shuffled = [...arr];
  for (let i = shuffled.length - 1; i > 0; i--) {
    const j = Math.floor(Math.random() * (i + 1));
    [shuffled[i], shuffled[j]] = [shuffled[j], shuffled[i]];
  }
  return shuffled;
}

function pickRandom<T>(arr: T[]): T {
  return arr[Math.floor(Math.random() * arr.length)];
}

/* ------------------------------------------------------------------ */
/*  Main Component                                                     */
/* ------------------------------------------------------------------ */

export default function LooneyTunesShow() {
  const rootRef = useRef<HTMLDivElement>(null);
  const artRef = useRef<HTMLDivElement>(null);
  const scopeRef = useRef<Scope | null>(null);
  const timerRef = useRef<ReturnType<typeof setInterval> | null>(null);
  const [actIndex, setActIndex] = useState(0);
  const [playlist] = useState(() => shuffleArray(ACTS));
  const [pokeCount, setPokeCount] = useState(0);
  const [pokeDialog, setPokeDialog] = useState<string | null>(null);
  const [isPoked, setIsPoked] = useState(false);

  const act = playlist[actIndex % playlist.length];

  // Auto-cycle acts every ~4s (reset timer on poke)
  const resetTimer = useCallback(() => {
    if (timerRef.current) clearInterval(timerRef.current);
    timerRef.current = setInterval(() => {
      setPokeDialog(null);
      setActIndex((i) => (i + 1) % playlist.length);
    }, 4000);
  }, [playlist.length]);

  useEffect(() => {
    resetTimer();
    return () => { if (timerRef.current) clearInterval(timerRef.current); };
  }, [resetTimer]);

  // Apply animations whenever act changes — use createScope for proper cleanup
  useEffect(() => {
    if (!artRef.current) return;

    if (scopeRef.current) {
      scopeRef.current.revert();
      scopeRef.current = null;
    }

    const el = artRef.current;

    requestAnimationFrame(() => {
      if (!artRef.current) return;
      scopeRef.current = createScope({ root: el }).add(() => {
        applyAnimations(el, act.anim);
      });
    });

    return () => {
      if (scopeRef.current) {
        scopeRef.current.revert();
        scopeRef.current = null;
      }
    };
  }, [act.anim, actIndex]);

  // Handle click / poke
  const handlePoke = useCallback(() => {
    const newCount = pokeCount + 1;
    setPokeCount(newCount);

    const egg = POKE_EASTER_EGGS.find((e) => e.threshold === newCount);
    if (egg) {
      const eggActIndex = playlist.findIndex((a) => a.character === egg.character);
      if (eggActIndex >= 0) setActIndex(eggActIndex);
      setPokeDialog(egg.line);
    } else {
      const reactions = POKE_REACTIONS[act.character];
      setPokeDialog(pickRandom(reactions));
    }

    if (artRef.current) {
      playPokeReaction(artRef.current);
    }

    setIsPoked(true);
    setTimeout(() => setIsPoked(false), 300);

    resetTimer();
  }, [pokeCount, act.character, playlist, resetTimer]);

  // Skip to next on double-click
  const handleDoubleClick = useCallback(() => {
    setPokeDialog(null);
    setActIndex((i) => (i + 1) % playlist.length);
    resetTimer();
  }, [playlist.length, resetTimer]);

  const displayLine = pokeDialog ?? act.line;

  return (
    <div
      ref={rootRef}
      className={`sidebar-toon-dancer${isPoked ? " toon-poked" : ""}`}
      title={`Click to poke ${CHAR_NAMES[act.character]}! Double-click to skip.`}
    >
      <div
        ref={artRef}
        className="toon-ascii-container"
        onClick={handlePoke}
        onDoubleClick={handleDoubleClick}
      >
        <AsciiCharacter character={act.character} anim={act.anim} />
      </div>
      <div className="toon-char-name">{CHAR_NAMES[act.character]}</div>
      <div className={`toon-dialog${pokeDialog ? " toon-dialog-poke" : ""}`}>{displayLine}</div>
      {pokeCount > 0 && (
        <div className="toon-poke-counter">Pokes: {pokeCount}</div>
      )}
    </div>
  );
}
