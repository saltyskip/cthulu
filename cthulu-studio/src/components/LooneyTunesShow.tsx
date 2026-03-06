import { useState, useEffect, useRef, useCallback } from "react";
import { animate, createScope, spring } from "animejs";
import type { Scope } from "animejs";

/* ------------------------------------------------------------------ */
/*  ACTS — 24+ random skits featuring Bugs, Daffy, & Elmer            */
/* ------------------------------------------------------------------ */

type Character = "bugs" | "daffy" | "elmer";

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
};

/** After this many pokes, unlock special easter-egg lines */
const POKE_EASTER_EGGS: { threshold: number; character: Character; line: string }[] = [
  { threshold: 5, character: "bugs", line: "You've poked me 5 times... What IS your deal, doc?" },
  { threshold: 10, character: "daffy", line: "TEN POKETH?! This is harassment! I'm calling my lawyer!" },
  { threshold: 15, character: "elmer", line: "15 pokes?! Even the wabbit doesn't bug me this much!" },
  { threshold: 20, character: "bugs", line: "20 pokes. Okay I respect the commitment. Wanna carrot?" },
  { threshold: 30, character: "daffy", line: "THIRTY?! You need a hobby. Theriouthly." },
  { threshold: 42, character: "elmer", line: "42 pokes! That's the meaning of wife... I mean LIFE!" },
  { threshold: 50, character: "bugs", line: "50 pokes, doc. You've officially lost your mind. Welcome!" },
];

const CHAR_NAMES: Record<Character, string> = {
  bugs: "Bugs Bunny",
  daffy: "Daffy Duck",
  elmer: "Elmer Fudd",
};

/* ------------------------------------------------------------------ */
/*  SVG Character Components                                          */
/* ------------------------------------------------------------------ */

function BugsBunnySVG() {
  return (
    <g id="toon-body">
      <defs>
        <linearGradient id="fur" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#a0a4aa" />
          <stop offset="100%" stopColor="#7a7e85" />
        </linearGradient>
        <linearGradient id="inner-ear" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#f5c78a" />
          <stop offset="100%" stopColor="#e8a44e" />
        </linearGradient>
      </defs>
      {/* Ears */}
      <g id="char-ear-l" style={{ transformOrigin: "44px 30px" }}>
        <ellipse cx="44" cy="14" rx="7" ry="22" fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
        <ellipse cx="44" cy="14" rx="3.5" ry="17" fill="url(#inner-ear)" />
      </g>
      <g id="char-ear-r" style={{ transformOrigin: "68px 30px" }}>
        <ellipse cx="68" cy="12" rx="7.5" ry="24" fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
        <ellipse cx="68" cy="12" rx="3.8" ry="19" fill="url(#inner-ear)" />
      </g>
      {/* Head */}
      <ellipse cx="56" cy="48" rx="22" ry="19" fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
      <ellipse cx="56" cy="54" rx="16" ry="13" fill="#fff" />
      <ellipse cx="42" cy="52" rx="8" ry="7" fill="#fff" />
      <ellipse cx="70" cy="52" rx="8" ry="7" fill="#fff" />
      {/* Eyes */}
      <ellipse cx="48" cy="44" rx="4" ry="5" fill="#fff" stroke="#222" strokeWidth="0.8" />
      <ellipse cx="64" cy="44" rx="4" ry="5" fill="#fff" stroke="#222" strokeWidth="0.8" />
      <circle cx="49.5" cy="44.5" r="2" fill="#111" />
      <circle cx="65.5" cy="44.5" r="2" fill="#111" />
      <circle cx="50.5" cy="43.5" r="0.7" fill="#fff" />
      <circle cx="66.5" cy="43.5" r="0.7" fill="#fff" />
      <path d="M44 41 Q48 39 52 41" fill="url(#fur)" stroke="#222" strokeWidth="0.6" />
      <path d="M60 41 Q64 39 68 41" fill="url(#fur)" stroke="#222" strokeWidth="0.6" />
      {/* Nose */}
      <ellipse cx="56" cy="50" rx="3" ry="2.2" fill="#e88" stroke="#222" strokeWidth="0.6" />
      {/* Whiskers */}
      <line x1="40" y1="50" x2="28" y2="47" stroke="#222" strokeWidth="0.5" />
      <line x1="40" y1="52" x2="27" y2="52" stroke="#222" strokeWidth="0.5" />
      <line x1="40" y1="54" x2="28" y2="57" stroke="#222" strokeWidth="0.5" />
      <line x1="72" y1="50" x2="84" y2="47" stroke="#222" strokeWidth="0.5" />
      <line x1="72" y1="52" x2="85" y2="52" stroke="#222" strokeWidth="0.5" />
      <line x1="72" y1="54" x2="84" y2="57" stroke="#222" strokeWidth="0.5" />
      {/* Mouth */}
      <g id="char-mouth" style={{ transformOrigin: "56px 58px" }}>
        <path d="M47 56 Q52 55 56 56 Q60 55 65 56 Q62 64 56 65 Q50 64 47 56Z"
          fill="#c0392b" stroke="#222" strokeWidth="0.8" />
        <ellipse cx="56" cy="62" rx="4" ry="2.5" fill="#e57373" />
      </g>
      <rect x="52" y="55.5" width="3.5" height="4.5" rx="1" fill="#fff" stroke="#222" strokeWidth="0.5" />
      <rect x="56" y="55.5" width="3.5" height="4.5" rx="1" fill="#fff" stroke="#222" strokeWidth="0.5" />
      {/* Body */}
      <ellipse cx="56" cy="82" rx="18" ry="20" fill="url(#fur)" stroke="#222" strokeWidth="1.2" />
      <ellipse cx="56" cy="84" rx="12" ry="15" fill="#fff" />
      {/* Left arm */}
      <g id="char-arm-l" style={{ transformOrigin: "38px 72px" }}>
        <path d="M38 72 Q30 78 28 86 Q27 88 30 88" fill="none" stroke="url(#fur)" strokeWidth="5" strokeLinecap="round" />
        <circle cx="29" cy="87" r="4" fill="#fff" stroke="#222" strokeWidth="0.8" />
      </g>
      {/* Right arm + carrot */}
      <g id="char-arm-r" style={{ transformOrigin: "74px 72px" }}>
        <path d="M74 72 Q82 64 84 58" fill="none" stroke="url(#fur)" strokeWidth="5" strokeLinecap="round" />
        <circle cx="84" cy="57" r="4" fill="#fff" stroke="#222" strokeWidth="0.8" />
        <g id="char-prop" style={{ transformOrigin: "88px 48px" }}>
          <polygon points="82,54 92,38 86,54" fill="#e67e22" stroke="#222" strokeWidth="0.6" />
          <path d="M91 38 Q89 32 86 30" fill="none" stroke="#27ae60" strokeWidth="1.5" strokeLinecap="round" />
          <path d="M92 38 Q93 33 91 29" fill="none" stroke="#27ae60" strokeWidth="1.5" strokeLinecap="round" />
          <path d="M92 39 Q96 34 95 30" fill="none" stroke="#2ecc71" strokeWidth="1" strokeLinecap="round" />
        </g>
      </g>
      {/* Legs */}
      <path d="M44 98 Q40 108 38 116" fill="none" stroke="url(#fur)" strokeWidth="6" strokeLinecap="round" />
      <path d="M68 98 Q72 108 74 116" fill="none" stroke="url(#fur)" strokeWidth="6" strokeLinecap="round" />
      {/* Feet */}
      <g id="char-foot-l" style={{ transformOrigin: "36px 120px" }}>
        <ellipse cx="32" cy="120" rx="10" ry="4" fill="url(#fur)" stroke="#222" strokeWidth="0.8" />
      </g>
      <ellipse cx="80" cy="120" rx="10" ry="4" fill="url(#fur)" stroke="#222" strokeWidth="0.8" />
      {/* Tail */}
      <circle cx="74" cy="96" r="4" fill="#fff" stroke="#ccc" strokeWidth="0.5" />
    </g>
  );
}

function DaffyDuckSVG() {
  return (
    <g id="toon-body">
      <defs>
        <linearGradient id="daffy-black" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#2d2d2d" />
          <stop offset="100%" stopColor="#111" />
        </linearGradient>
      </defs>
      {/* Head feather tuft */}
      <g id="char-ear-l" style={{ transformOrigin: "56px 20px" }}>
        <path d="M52 22 Q48 8 44 4 Q46 10 50 16" fill="#111" stroke="#222" strokeWidth="0.6" />
        <path d="M56 20 Q54 6 56 0 Q56 8 57 14" fill="#111" stroke="#222" strokeWidth="0.6" />
        <path d="M60 22 Q64 10 68 6 Q64 12 62 18" fill="#111" stroke="#222" strokeWidth="0.6" />
      </g>
      <g id="char-ear-r" style={{ transformOrigin: "56px 20px" }} />
      {/* Head */}
      <ellipse cx="56" cy="38" rx="20" ry="18" fill="url(#daffy-black)" stroke="#222" strokeWidth="1.2" />
      {/* Bill */}
      <g id="char-mouth" style={{ transformOrigin: "56px 52px" }}>
        <ellipse cx="56" cy="48" rx="18" ry="8" fill="#e8a832" stroke="#222" strokeWidth="1" />
        <path d="M38 48 Q56 46 74 48" fill="none" stroke="#c68a20" strokeWidth="0.8" />
        <ellipse cx="56" cy="52" rx="14" ry="5" fill="#d4922a" stroke="#222" strokeWidth="0.8" />
        <ellipse cx="56" cy="51" rx="10" ry="3" fill="#c0392b" />
      </g>
      {/* Eyes */}
      <ellipse cx="47" cy="34" rx="6" ry="7" fill="#fff" stroke="#222" strokeWidth="0.8" />
      <ellipse cx="65" cy="34" rx="6" ry="7" fill="#fff" stroke="#222" strokeWidth="0.8" />
      <circle cx="49" cy="35" r="2.5" fill="#111" />
      <circle cx="67" cy="35" r="2.5" fill="#111" />
      <circle cx="50" cy="34" r="0.8" fill="#fff" />
      <circle cx="68" cy="34" r="0.8" fill="#fff" />
      <path d="M41 30 Q47 28 53 31" fill="url(#daffy-black)" stroke="#222" strokeWidth="0.6" />
      <path d="M59 31 Q65 28 71 30" fill="url(#daffy-black)" stroke="#222" strokeWidth="0.6" />
      {/* Neck ring */}
      <ellipse cx="56" cy="56" rx="10" ry="4" fill="#fff" stroke="#222" strokeWidth="0.6" />
      {/* Body */}
      <ellipse cx="56" cy="80" rx="16" ry="22" fill="url(#daffy-black)" stroke="#222" strokeWidth="1.2" />
      {/* Tail feathers */}
      <path d="M72 92 Q80 88 82 82 Q78 90 74 92" fill="#111" stroke="#222" strokeWidth="0.5" />
      <path d="M70 96 Q78 94 82 88 Q76 94 72 97" fill="#111" stroke="#222" strokeWidth="0.5" />
      {/* Left arm */}
      <g id="char-arm-l" style={{ transformOrigin: "40px 70px" }}>
        <path d="M40 70 Q32 76 30 82" fill="none" stroke="url(#daffy-black)" strokeWidth="4" strokeLinecap="round" />
        <path d="M30 82 Q28 84 26 82 Q28 80 30 82" fill="#111" stroke="#222" strokeWidth="0.5" />
        <path d="M31 83 Q29 86 27 84 Q29 82 31 83" fill="#111" stroke="#222" strokeWidth="0.5" />
      </g>
      {/* Right arm */}
      <g id="char-arm-r" style={{ transformOrigin: "72px 70px" }}>
        <path d="M72 70 Q80 64 82 58" fill="none" stroke="url(#daffy-black)" strokeWidth="4" strokeLinecap="round" />
        <path d="M82 58 Q84 56 86 58 Q84 60 82 58" fill="#111" stroke="#222" strokeWidth="0.5" />
        <path d="M83 59 Q85 57 87 60 Q85 61 83 59" fill="#111" stroke="#222" strokeWidth="0.5" />
        <g id="char-prop" />
      </g>
      {/* Legs */}
      <line x1="48" y1="100" x2="44" y2="116" stroke="#e8a832" strokeWidth="3" strokeLinecap="round" />
      <line x1="64" y1="100" x2="68" y2="116" stroke="#e8a832" strokeWidth="3" strokeLinecap="round" />
      {/* Feet */}
      <g id="char-foot-l" style={{ transformOrigin: "40px 120px" }}>
        <path d="M30 120 Q38 116 44 120 Q38 122 30 120Z" fill="#e8a832" stroke="#222" strokeWidth="0.8" />
        <path d="M34 118 Q38 114 42 118" fill="#e8a832" stroke="#c68a20" strokeWidth="0.5" />
      </g>
      <path d="M68 120 Q76 116 82 120 Q76 122 68 120Z" fill="#e8a832" stroke="#222" strokeWidth="0.8" />
    </g>
  );
}

function ElmerFuddSVG() {
  return (
    <g id="toon-body">
      <defs>
        <linearGradient id="elmer-jacket" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#c98432" />
          <stop offset="100%" stopColor="#a06820" />
        </linearGradient>
        <linearGradient id="elmer-hat" x1="0" y1="0" x2="0" y2="1">
          <stop offset="0%" stopColor="#b57530" />
          <stop offset="100%" stopColor="#8a5a20" />
        </linearGradient>
      </defs>
      {/* Hat */}
      <g id="char-ear-l" style={{ transformOrigin: "56px 24px" }}>
        <ellipse cx="56" cy="22" rx="22" ry="10" fill="url(#elmer-hat)" stroke="#222" strokeWidth="1" />
        <ellipse cx="56" cy="18" rx="16" ry="14" fill="url(#elmer-hat)" stroke="#222" strokeWidth="1" />
        <path d="M40 18 Q56 10 72 18" fill="#c98432" stroke="#222" strokeWidth="0.6" />
        <rect x="42" y="20" width="28" height="4" rx="2" fill="#c0392b" />
        <path d="M40 22 Q36 28 38 34" fill="url(#elmer-hat)" stroke="#222" strokeWidth="0.8" />
        <path d="M72 22 Q76 28 74 34" fill="url(#elmer-hat)" stroke="#222" strokeWidth="0.8" />
      </g>
      <g id="char-ear-r" style={{ transformOrigin: "56px 24px" }} />
      {/* Head */}
      <ellipse cx="56" cy="42" rx="20" ry="18" fill="#fdd9b5" stroke="#222" strokeWidth="1" />
      <ellipse cx="42" cy="48" rx="5" ry="3" fill="#f0b0a0" opacity="0.5" />
      <ellipse cx="70" cy="48" rx="5" ry="3" fill="#f0b0a0" opacity="0.5" />
      {/* Eyes */}
      <ellipse cx="49" cy="40" rx="4" ry="4.5" fill="#fff" stroke="#222" strokeWidth="0.8" />
      <ellipse cx="63" cy="40" rx="4" ry="4.5" fill="#fff" stroke="#222" strokeWidth="0.8" />
      <circle cx="50" cy="40.5" r="2" fill="#111" />
      <circle cx="64" cy="40.5" r="2" fill="#111" />
      <circle cx="51" cy="39.5" r="0.6" fill="#fff" />
      <circle cx="65" cy="39.5" r="0.6" fill="#fff" />
      <path d="M45 36 Q49 34 53 36" fill="none" stroke="#222" strokeWidth="0.8" />
      <path d="M59 36 Q63 34 67 36" fill="none" stroke="#222" strokeWidth="0.8" />
      {/* Nose */}
      <circle cx="56" cy="46" r="3" fill="#f0a090" stroke="#222" strokeWidth="0.6" />
      {/* Mouth */}
      <g id="char-mouth" style={{ transformOrigin: "56px 52px" }}>
        <path d="M49 52 Q53 50 56 52 Q59 50 63 52 Q60 56 56 57 Q52 56 49 52Z"
          fill="#c0392b" stroke="#222" strokeWidth="0.6" />
      </g>
      <ellipse cx="56" cy="58" rx="8" ry="4" fill="#fdd9b5" stroke="none" />
      {/* Body */}
      <ellipse cx="56" cy="82" rx="18" ry="22" fill="url(#elmer-jacket)" stroke="#222" strokeWidth="1.2" />
      <path d="M44 64 Q56 60 68 64" fill="url(#elmer-jacket)" stroke="#222" strokeWidth="0.8" />
      <line x1="56" y1="64" x2="56" y2="100" stroke="#222" strokeWidth="0.5" />
      <circle cx="56" cy="72" r="1.2" fill="#8a5a20" stroke="#222" strokeWidth="0.3" />
      <circle cx="56" cy="80" r="1.2" fill="#8a5a20" stroke="#222" strokeWidth="0.3" />
      <circle cx="56" cy="88" r="1.2" fill="#8a5a20" stroke="#222" strokeWidth="0.3" />
      {/* Left arm */}
      <g id="char-arm-l" style={{ transformOrigin: "40px 72px" }}>
        <path d="M40 72 Q34 78 32 84" fill="none" stroke="url(#elmer-jacket)" strokeWidth="5" strokeLinecap="round" />
        <circle cx="32" cy="85" r="3.5" fill="#fdd9b5" stroke="#222" strokeWidth="0.6" />
      </g>
      {/* Right arm + rifle */}
      <g id="char-arm-r" style={{ transformOrigin: "72px 72px" }}>
        <path d="M72 72 Q78 66 80 60" fill="none" stroke="url(#elmer-jacket)" strokeWidth="5" strokeLinecap="round" />
        <circle cx="80" cy="59" r="3.5" fill="#fdd9b5" stroke="#222" strokeWidth="0.6" />
        <g id="char-prop" style={{ transformOrigin: "80px 55px" }}>
          <line x1="78" y1="58" x2="68" y2="98" stroke="#666" strokeWidth="2.5" strokeLinecap="round" />
          <line x1="78" y1="58" x2="92" y2="34" stroke="#888" strokeWidth="2" strokeLinecap="round" />
          <path d="M66 96 Q64 102 68 104 Q72 102 70 96Z" fill="#8B4513" stroke="#222" strokeWidth="0.6" />
          <circle cx="92" cy="33" r="1.5" fill="#999" stroke="#222" strokeWidth="0.4" />
        </g>
      </g>
      {/* Legs */}
      <path d="M46 100 Q44 110 42 118" fill="none" stroke="url(#elmer-jacket)" strokeWidth="6" strokeLinecap="round" />
      <path d="M66 100 Q68 110 70 118" fill="none" stroke="url(#elmer-jacket)" strokeWidth="6" strokeLinecap="round" />
      {/* Boots */}
      <g id="char-foot-l" style={{ transformOrigin: "38px 120px" }}>
        <ellipse cx="38" cy="122" rx="8" ry="4" fill="#8B2020" stroke="#222" strokeWidth="0.8" />
      </g>
      <ellipse cx="74" cy="122" rx="8" ry="4" fill="#8B2020" stroke="#222" strokeWidth="0.8" />
    </g>
  );
}

/* ------------------------------------------------------------------ */
/*  Animation engine (anime.js v4)                                     */
/* ------------------------------------------------------------------ */

function applyAnimations(svg: SVGSVGElement, anim: string): void {
  const q = (sel: string) => svg.querySelector(sel) as SVGElement | null;

  // Base idle bounce (always)
  animate(q("#toon-body")!, {
    translateY: [-2, 2],
    duration: 500 + Math.random() * 200,
    ease: "inOutSine",
    alternate: true,
    loop: true,
  });

  // Ear/hat/tuft wiggle (always)
  animate(q("#char-ear-l")!, {
    rotate: [-8, 8],
    duration: 700,
    ease: "inOutQuad",
    alternate: true,
    loop: true,
  });

  switch (anim) {
    case "chomp":
    case "laugh":
      animate(q("#char-mouth")!, { scaleY: [1, 0.4, 1], duration: 500, ease: "inOutSine", loop: true });
      animate(q("#char-arm-r")!, { rotate: [0, -15, 0], duration: 800, ease: "inOutSine", loop: true });
      break;
    case "wave":
    case "point":
      animate(q("#char-arm-r")!, { rotate: [0, -30, 0, -30, 0], duration: 1200, ease: "inOutSine", loop: true });
      break;
    case "smug":
    case "strut":
      animate(q("#toon-body")!, { rotate: [-2, 2], duration: 800, ease: "inOutSine", alternate: true, loop: true });
      break;
    case "shrug":
    case "confused":
      animate(q("#char-arm-l")!, { rotate: [0, 15, 0], duration: 1000, ease: "inOutSine", loop: true });
      animate(q("#char-arm-r")!, { rotate: [0, -15, 0], duration: 1000, ease: "inOutSine", loop: true });
      break;
    case "dance":
    case "bounce":
      animate(q("#toon-body")!, { translateY: [-6, 6], rotate: [-3, 3], duration: 400, ease: "inOutSine", alternate: true, loop: true });
      animate(q("#char-foot-l")!, { rotate: [-10, 10], duration: 300, ease: "inOutSine", alternate: true, loop: true });
      animate(q("#char-arm-l")!, { rotate: [0, 20, 0, -10, 0], duration: 800, ease: "inOutSine", loop: true });
      animate(q("#char-arm-r")!, { rotate: [0, -20, 0, 10, 0], duration: 800, ease: "inOutSine", loop: true });
      break;
    case "rage":
      animate(q("#toon-body")!, { translateX: [-3, 3], duration: 150, ease: "inOutSine", alternate: true, loop: true });
      animate(q("#char-arm-l")!, { rotate: [0, 20, 0], duration: 400, ease: "inOutSine", loop: true });
      animate(q("#char-arm-r")!, { rotate: [0, -20, 0], duration: 400, ease: "inOutSine", loop: true });
      break;
    case "grab":
      animate(q("#char-arm-r")!, { rotate: [0, -35, -10, -35, 0], duration: 1000, ease: "inOutSine", loop: true });
      animate(q("#char-arm-l")!, { rotate: [0, 20, 5, 20, 0], duration: 1000, ease: "inOutSine", loop: true });
      break;
    case "dodge":
      animate(q("#toon-body")!, { translateX: [-8, 8], rotate: [-5, 5], duration: 600, ease: "inOutSine", alternate: true, loop: true });
      break;
    case "sneak":
      animate(q("#toon-body")!, { translateX: [-4, 4], translateY: [0, 3, 0], duration: 1200, ease: "inOutSine", alternate: true, loop: true });
      animate(q("#char-foot-l")!, { rotate: [-5, 5], duration: 600, ease: "inOutSine", alternate: true, loop: true });
      break;
    case "aim":
      animate(q("#char-arm-r")!, { rotate: [0, -10, -5, -10, 0], duration: 1500, ease: "inOutSine", loop: true });
      animate(q("#char-prop")!, { rotate: [-3, 3], duration: 800, ease: "inOutSine", alternate: true, loop: true });
      break;
    case "chase":
      animate(q("#toon-body")!, { translateX: [-6, 6], duration: 400, ease: "inOutSine", alternate: true, loop: true });
      animate(q("#char-foot-l")!, { rotate: [-12, 12], duration: 250, ease: "inOutSine", alternate: true, loop: true });
      animate(q("#char-arm-l")!, { rotate: [-10, 10], duration: 300, ease: "inOutSine", alternate: true, loop: true });
      break;
    default:
      animate(q("#char-arm-r")!, { rotate: [0, -10, 0], duration: 1500, ease: "inOutSine", loop: true });
      break;
  }
}

/** One-shot "poked!" squish animation */
function playPokeReaction(svg: SVGSVGElement): void {
  animate(svg.querySelector("#toon-body")!, {
    scaleX: [1, 1.15, 0.9, 1.05, 1],
    scaleY: [1, 0.85, 1.1, 0.95, 1],
    duration: 500,
    ease: spring({ stiffness: 300, damping: 12 }),
  });
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
  const svgRef = useRef<SVGSVGElement>(null);
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
    if (!svgRef.current) return;

    // Revert previous scope (stops all animations created within it)
    if (scopeRef.current) {
      scopeRef.current.revert();
      scopeRef.current = null;
    }

    const svg = svgRef.current;

    // Small delay to ensure SVG elements are rendered after character swap
    requestAnimationFrame(() => {
      if (!svgRef.current) return;
      scopeRef.current = createScope({ root: svg }).add(() => {
        applyAnimations(svg, act.anim);
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

    // Check for easter egg first
    const egg = POKE_EASTER_EGGS.find((e) => e.threshold === newCount);
    if (egg) {
      // Jump to the easter egg character
      const eggActIndex = playlist.findIndex((a) => a.character === egg.character);
      if (eggActIndex >= 0) setActIndex(eggActIndex);
      setPokeDialog(egg.line);
    } else {
      // Pick a random poke reaction for the CURRENT character
      const reactions = POKE_REACTIONS[act.character];
      setPokeDialog(pickRandom(reactions));
    }

    // Play squish animation
    if (svgRef.current) {
      playPokeReaction(svgRef.current);
    }

    // Visual poke flash
    setIsPoked(true);
    setTimeout(() => setIsPoked(false), 300);

    // Reset auto-cycle timer so they get to read the poke line
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
      <svg
        ref={svgRef}
        viewBox="0 0 120 150"
        width="96"
        height="120"
        xmlns="http://www.w3.org/2000/svg"
        className="toon-svg"
        onClick={handlePoke}
        onDoubleClick={handleDoubleClick}
      >
        {act.character === "bugs" && <BugsBunnySVG />}
        {act.character === "daffy" && <DaffyDuckSVG />}
        {act.character === "elmer" && <ElmerFuddSVG />}
      </svg>
      <div className="toon-char-name">{CHAR_NAMES[act.character]}</div>
      <div className={`toon-dialog${pokeDialog ? " toon-dialog-poke" : ""}`}>{displayLine}</div>
      {pokeCount > 0 && (
        <div className="toon-poke-counter">Pokes: {pokeCount}</div>
      )}
    </div>
  );
}
