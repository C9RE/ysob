<script>
  import { S } from './lib/store.svelte.js';
  import { PK, url, member } from './lib/packdata.svelte.js';

  /** The big moment: the hunter decloaks, three dots lock on, the line slams in.
   *  A handshake (new mission or squadmate) shows the squad member instead.
   *  It never takes a click or a key from the work underneath; Esc waves it away (App.svelte). */
  const hunter = $derived(PK.hunter.portrait ? url(PK.hunter.portrait) : '');
  const friend = $derived.by(() => {
    const slug = member(S.strike?.who ?? '')?.slug ?? '';
    return PK.crew[slug]?.portrait ? url(/** @type {string} */ (PK.crew[slug].portrait)) : '';
  });
</script>

{#if S.strike}
  {#key S.strike}
    <div class="strike" class:lie={S.strike.kind === 'lie'} class:join={S.strike.kind === 'join'} role="alert" aria-live="assertive">
      <div class="veil"></div>
      {#if S.strike.kind === 'join' && friend}
        <img class="hunter friend pixel" src={friend} alt="" />
      {:else if hunter && S.strike.kind === 'hit'}
        <img class="hunter pixel" src={hunter} alt="" />
      {/if}
      {#if S.strike.kind !== 'join'}<div class="dots" aria-hidden="true"><i></i><i></i><i></i></div>{/if}
      <div class="words">
        <span class="line">{S.strike.line}</span>
        <span class="who">{S.strike.sub}</span>
      </div>
    </div>
  {/key}
{/if}

<style>
  .strike {
    position: fixed;
    inset: 0;
    z-index: 80;
    display: grid;
    place-items: center;
    pointer-events: none;
  }
  .veil {
    position: absolute;
    inset: 0;
    background: radial-gradient(ellipse at center, rgb(28 0 0 / 0.78), rgb(5 0 0 / 0.9) 70%);
    animation: veil 2.8s ease forwards;
  }
  @keyframes veil { 0% { opacity: 0; } 8% { opacity: 1; } 82% { opacity: 1; } 100% { opacity: 0; } }
  .hunter {
    position: absolute;
    height: min(62vh, 520px);
    bottom: 0;
    right: 2vw;
    opacity: 0;
    animation: decloak 2.8s ease forwards;
  }
  @keyframes decloak {
    0% { opacity: 0; filter: contrast(3) hue-rotate(160deg) blur(3px); transform: translateX(30px) skewX(-6deg); }
    15% { opacity: 0.25; filter: contrast(3) hue-rotate(160deg) blur(1px); }
    22% { opacity: 0.9; filter: brightness(2.2) saturate(0); transform: none; }
    26% { opacity: 1; filter: none; }
    82% { opacity: 1; }
    100% { opacity: 0; }
  }
  .friend { left: 8vw; right: auto; animation: arrive 2.8s ease forwards; }
  @keyframes arrive {
    0% { opacity: 0; transform: translateX(-60px); }
    18% { opacity: 1; transform: none; }
    82% { opacity: 1; }
    100% { opacity: 0; }
  }
  .join .veil { background: radial-gradient(ellipse at center, rgb(6 20 12 / 0.78), rgb(2 6 4 / 0.9) 70%); }
  .dots { position: absolute; width: 60px; height: 54px; }
  .dots i {
    position: absolute;
    width: 12px;
    height: 12px;
    border-radius: 50%;
    background: #ff2a1a;
    box-shadow: 0 0 18px 6px rgb(255 42 26 / 0.7);
    animation: converge 2.8s cubic-bezier(0.2, 0.8, 0.2, 1) forwards;
  }
  .dots i:nth-child(1) { left: 24px; top: 0; --x: -40vw; --y: -30vh; }
  .dots i:nth-child(2) { left: 0; top: 40px; --x: -30vw; --y: 40vh; animation-delay: 0.06s; }
  .dots i:nth-child(3) { left: 48px; top: 40px; --x: 40vw; --y: 20vh; animation-delay: 0.12s; }
  @keyframes converge {
    0% { transform: translate(var(--x), var(--y)); opacity: 0; }
    10% { opacity: 1; }
    30% { transform: none; }
    82% { opacity: 1; transform: none; }
    100% { opacity: 0; }
  }
  .words {
    position: relative;
    display: grid;
    justify-items: center;
    gap: 10px;
    margin-top: 150px;
  }
  .line {
    font: 600 clamp(36px, 7vw, 92px) / 0.95 var(--font-pixel);
    text-wrap: balance;
    padding: 0 16px;
    letter-spacing: 0.02em;
    text-align: center;
    color: #fff;
    text-shadow: 0 0 30px rgb(255 60 40 / 0.6), 4px 4px 0 #a5110a;
    animation: slam 2.8s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }
  @keyframes slam {
    0%, 26% { opacity: 0; transform: scale(2.4); }
    34% { opacity: 1; transform: scale(1); }
    38% { transform: scale(1.04) rotate(-1deg); }
    42% { transform: scale(1); }
    82% { opacity: 1; }
    100% { opacity: 0; }
  }
  .who {
    font-size: 15px;
    font-weight: 600;
    text-align: center;
    padding: 0 16px;
    color: rgb(255 222 214 / 0.95);
    animation: slam 2.8s ease forwards;
  }
  .lie .line {
    color: #ff4b3a;
    text-shadow: none;
    padding: 6px 22px 10px;
    border: 6px solid #ff4b3a;
    border-radius: 10px;
    transform: rotate(-6deg);
    animation: stamp 2.8s cubic-bezier(0.16, 1, 0.3, 1) forwards;
  }
  @keyframes stamp {
    0%, 6% { opacity: 0; transform: rotate(-6deg) scale(3); }
    14% { opacity: 1; transform: rotate(-6deg) scale(1); }
    82% { opacity: 1; }
    100% { opacity: 0; }
  }
</style>
