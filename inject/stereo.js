// Audio micro de Discord web, réglable depuis Paramètres Plume.
//
// Le client web passe par le WebRTC du navigateur : pas de discord_voice.node à
// patcher. Ce qu'on contrôle :
//   1. la capture : 2 canaux, et chaque traitement (annulation d'écho,
//      suppression de bruit, gain auto) activé ou non. Les trois remixent en
//      mono : pour la stéréo, il faut les couper ;
//   2. la négociation SDP de la ligne Opus : stereo, débit, CBR, FEC, DTX,
//      taille de trame. Ces paramètres sont fixés par le récepteur, donc on
//      les écrit dans les descriptions locale ET distante ;
//   3. l'émetteur : débit maximal et priorité réseau.
//
// Les traitements du micro, le débit et la priorité s'appliquent en direct
// (__plumeApply) ; le reste à la prochaine connexion vocale.
// Console : plumeStereo.status(), plumeStereo.selfTest().
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
    const log = (...a) => console.log("%c[Plume audio]", "color:#8b7cf6;font-weight:bold", ...a);

    let A = { ...(window.__PLUME__?.audio || {}) };
    // __PLUME__ date de la création de la fenêtre : on relit la version à jour.
    invoke("audio_config").then(c => (A = c)).catch(() => {});

    // ---- 2. SDP ----------------------------------------------------------
    function opusParams() {
        const p = {
            maxaveragebitrate: String(A.bitrate),
            maxplaybackrate: "48000",
            "sprop-maxcapturerate": "48000",
            cbr: A.cbr ? "1" : "0",
            useinbandfec: A.fec ? "1" : "0",
            usedtx: A.dtx ? "1" : "0",
            minptime: String(Math.min(10, A.frameMs)),
        };
        if (A.stereo) Object.assign(p, { stereo: "1", "sprop-stereo": "1" });
        return p;
    }

    function mungeSdp(sdp) {
        if (typeof sdp !== "string") return sdp;
        const nl = sdp.includes("\r\n") ? "\r\n" : "\n";
        const pts = [...sdp.matchAll(/a=rtpmap:(\d+) opus\/48000\/2/gi)].map(m => m[1]);
        const want = opusParams();
        for (const pt of pts) {
            const fmtp = new RegExp(`a=fmtp:${pt} ([^\\r\\n]*)`);
            const params = new Map();
            const m = fmtp.exec(sdp);
            if (m) for (const kv of m[1].split(";").map(s => s.trim()).filter(Boolean)) {
                const i = kv.indexOf("=");
                params.set(i < 0 ? kv : kv.slice(0, i), i < 0 ? "" : kv.slice(i + 1));
            }
            if (!A.stereo) { params.delete("stereo"); params.delete("sprop-stereo"); }
            for (const [k, v] of Object.entries(want)) params.set(k, v);
            const line = `a=fmtp:${pt} ${[...params].map(([k, v]) => (v === "" ? k : `${k}=${v}`)).join(";")}`;
            sdp = m ? sdp.replace(fmtp, line) : sdp.replace(new RegExp(`(a=rtpmap:${pt} opus/48000/2[^\\r\\n]*)`, "i"), `$1${nl}${line}`);
        }
        // Taille de trame : a=ptime / a=maxptime de chaque section audio.
        if (pts.length) {
            sdp = sdp.replace(/^a=(max)?ptime:\d+\r?\n/gm, "");
            sdp = sdp.replace(/(m=audio [^\r\n]*\r?\n)/g, `$1a=ptime:${A.frameMs}${nl}a=maxptime:${A.frameMs}${nl}`);
        }
        return sdp;
    }

    // ---- 1. Micro ----------------------------------------------------------
    const PROCESSING = ["echoCancellation", "noiseSuppression", "autoGainControl"];

    function micConstraints(audio) {
        const a = typeof audio === "object" && audio ? { ...audio } : {};
        // Anciennes contraintes Chrome (googEchoCancellation…) : on les retire.
        for (const k of Object.keys(a)) if (k.startsWith("goog")) delete a[k];
        if (a.mandatory) a.mandatory = Object.fromEntries(Object.entries(a.mandatory).filter(([k]) => !k.startsWith("goog")));
        if (Array.isArray(a.optional)) a.optional = a.optional.filter(o => !Object.keys(o).some(k => k.startsWith("goog")));
        if (Array.isArray(a.advanced)) a.advanced = a.advanced.map(o => Object.fromEntries(Object.entries(o).filter(([k]) => !PROCESSING.includes(k) && k !== "channelCount")));
        if (A.stereo) a.channelCount = { ideal: 2 };
        a.sampleRate = { ideal: 48000 };
        for (const k of PROCESSING) a[k] = !!A[k];
        return a;
    }

    const peers = new Set();
    const mics = new Set();

    const md = navigator.mediaDevices;
    const getUserMedia = md.getUserMedia.bind(md);
    md.getUserMedia = async function (constraints) {
        if (constraints?.audio) constraints = { ...constraints, audio: micConstraints(constraints.audio) };
        const stream = await getUserMedia(constraints);
        stream.getAudioTracks().forEach(t => mics.add(t));
        return stream;
    };

    const P = RTCPeerConnection.prototype;
    for (const name of ["createOffer", "createAnswer"]) {
        const orig = P[name];
        P[name] = async function (...args) {
            const desc = await orig.apply(this, args);
            return desc?.sdp ? new RTCSessionDescription({ type: desc.type, sdp: mungeSdp(desc.sdp) }) : desc;
        };
    }
    for (const name of ["setLocalDescription", "setRemoteDescription"]) {
        const orig = P[name];
        P[name] = function (desc, ...rest) {
            peers.add(this);
            if (desc?.sdp) desc = { type: desc.type, sdp: mungeSdp(desc.sdp) };
            return orig.call(this, desc, ...rest);
        };
    }

    // ---- 3. Émetteur -------------------------------------------------------
    function tuneEncodings(params) {
        for (const e of params.encodings || []) {
            e.maxBitrate = A.bitrate;
            if (A.highPriority) { e.priority = "high"; e.networkPriority = "high"; }
        }
        return params;
    }
    const setParameters = RTCRtpSender.prototype.setParameters;
    RTCRtpSender.prototype.setParameters = function (params, ...rest) {
        if (this.track?.kind === "audio" && params?.encodings?.length) tuneEncodings(params);
        return setParameters.call(this, params, ...rest);
    };

    function audioSenders() {
        const out = [];
        for (const pc of peers) {
            if (pc.connectionState === "closed") { peers.delete(pc); continue; }
            for (const s of pc.getSenders()) if (s.track?.kind === "audio") out.push([pc, s]);
        }
        return out;
    }

    // Réglages modifiés dans Paramètres Plume, appliqués sans recharger.
    window.__plumeApply = async next => {
        A = { ...next };
        for (const t of mics) {
            if (t.readyState === "ended") { mics.delete(t); continue; }
            await t.applyConstraints(Object.fromEntries(PROCESSING.map(k => [k, !!A[k]]))).catch(() => {});
        }
        for (const [, s] of audioSenders()) {
            const p = s.getParameters();
            if (p.encodings?.length) await setParameters.call(s, tuneEncodings(p)).catch(() => {});
        }
        log("réglages appliqués", A);
    };

    // ---- Diagnostic --------------------------------------------------------
    const last = new WeakMap();
    async function status() {
        const out = [];
        for (const [pc, sender] of audioSenders()) {
            const s = sender.track.getSettings();
            const stats = await pc.getStats();
            const codecs = {};
            stats.forEach(r => r.type === "codec" && (codecs[r.id] = r));
            stats.forEach(r => {
                if (r.type !== "outbound-rtp" || r.kind !== "audio") return;
                const prev = last.get(sender);
                last.set(sender, { t: r.timestamp, bytes: r.bytesSent, packets: r.packetsSent });
                const dt = prev ? (r.timestamp - prev.t) / 1000 : 0;
                const codec = codecs[r.codecId];
                out.push({
                    canauxMicro: s.channelCount,
                    echoCancellation: s.echoCancellation,
                    noiseSuppression: s.noiseSuppression,
                    autoGainControl: s.autoGainControl,
                    codec: codec && `${codec.mimeType} ${codec.channels} canaux`,
                    fmtp: codec?.sdpFmtpLine,
                    debitCible: r.targetBitrate,
                    debitReel: dt > 0 ? Math.round(((r.bytesSent - prev.bytes) * 8) / dt) : null,
                    paquetsParSeconde: dt > 0 ? Math.round((r.packetsSent - prev.packets) / dt) : null,
                    stereo: s.channelCount === 2 && /stereo=1/.test(codec?.sdpFmtpLine || ""),
                });
            });
        }
        return out;
    }

    // Boucle locale : un son à gauche seulement, envoyé d'une connexion WebRTC
    // à une autre à travers les mêmes réglages. Mesure aussi le débit réel et
    // la taille de trame (paquets par seconde).
    async function selfTest(seconds = 3) {
        const ctx = new AudioContext({ sampleRate: 48000 });
        const noise = ctx.createBufferSource();
        const buffer = ctx.createBuffer(1, 48000, 48000);
        const ch = buffer.getChannelData(0);
        for (let i = 0; i < ch.length; i++) ch[i] = (Math.random() * 2 - 1) * 0.5;
        noise.buffer = buffer;
        noise.loop = true;
        const merger = ctx.createChannelMerger(2);
        noise.connect(merger, 0, 0);
        const dest = ctx.createMediaStreamDestination();
        dest.channelCount = 2;
        merger.connect(dest);
        noise.start();

        const a = new RTCPeerConnection(), b = new RTCPeerConnection();
        a.onicecandidate = e => e.candidate && b.addIceCandidate(e.candidate);
        b.onicecandidate = e => e.candidate && a.addIceCandidate(e.candidate);
        const remote = new Promise(r => (b.ontrack = e => r(e.streams[0] || new MediaStream([e.track]))));
        const sender = a.addTrack(dest.stream.getAudioTracks()[0], dest.stream);
        await a.setLocalDescription(await a.createOffer());
        await b.setRemoteDescription(a.localDescription);
        await b.setLocalDescription(await b.createAnswer());
        await a.setRemoteDescription(b.localDescription);
        const p = sender.getParameters();
        if (p.encodings?.length) await sender.setParameters(p).catch(() => {});

        const stream = await remote;
        // Chrome ne fait couler l'audio WebRTC distant dans WebAudio que si un
        // élément <audio> le consomme.
        const el = new Audio();
        el.muted = true;
        el.srcObject = stream;
        el.play().catch(() => {});
        const split = ctx.createChannelSplitter(2);
        ctx.createMediaStreamSource(stream).connect(split);
        const analysers = [0, 1].map(i => {
            const an = ctx.createAnalyser();
            an.fftSize = 2048;
            split.connect(an, i);
            return an;
        });
        const buf = new Float32Array(2048);
        const rms = [0, 0];
        let n = 0;
        const sample = async () => {
            let r;
            (await a.getStats()).forEach(s => s.type === "outbound-rtp" && s.kind === "audio" && (r = s));
            return r;
        };
        await new Promise(r => setTimeout(r, 500));
        const s0 = await sample();
        const end = performance.now() + seconds * 1000;
        while (performance.now() < end) {
            await new Promise(r => setTimeout(r, 50));
            analysers.forEach((an, i) => {
                an.getFloatTimeDomainData(buf);
                rms[i] += Math.sqrt(buf.reduce((s, v) => s + v * v, 0) / buf.length);
            });
            n++;
        }
        const s1 = await sample();
        const dt = (s1.timestamp - s0.timestamp) / 1000;
        let fmtp;
        (await b.getStats()).forEach(s => s.type === "codec" && /opus/i.test(s.mimeType) && (fmtp = s.sdpFmtpLine));
        a.close(); b.close(); noise.stop(); ctx.close(); el.srcObject = null;
        const [left, right] = rms.map(v => v / n);
        const pps = (s1.packetsSent - s0.packetsSent) / dt;
        const result = {
            gauche: +left.toFixed(4),
            droite: +right.toFixed(4),
            stereo: left > 0.05 && right < left * 0.1,
            debitReel: Math.round(((s1.bytesSent - s0.bytesSent) * 8) / dt),
            trameMs: pps > 0 ? Math.round(1000 / pps) : null,
            fmtp,
        };
        log("auto-test", result);
        return result;
    }

    // Demandé par la fenêtre Paramètres Plume (via Rust).
    window.__plumeProbe = async kind => {
        let data;
        try {
            data = kind === "test" ? await selfTest(3) : await status();
        } catch (e) {
            data = { erreur: String(e) };
        }
        invoke("audio_report", { data }).catch(() => {});
    };

    window.plumeStereo = { mungeSdp, status, selfTest, get settings() { return A; } };
    log("actif", A);
})();
