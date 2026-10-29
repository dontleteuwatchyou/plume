// Stéréo micro pour Discord web.
//
// Le client web passe par le WebRTC du navigateur : pas de discord_voice.node à
// patcher. Pour que l'Opus parte en stéréo il faut trois choses :
//   1. une piste micro à 2 canaux, sans les traitements qui la remixent en mono
//      (annulation d'écho, suppression de bruit, gain auto) ;
//   2. stereo=1 dans la négociation SDP : c'est le récepteur qui le demande,
//      donc on le met dans les deux descriptions, locale et distante ;
//   3. un débit à la hauteur (maxaveragebitrate + maxBitrate de l'émetteur).
//
// Diagnostic dans la console : plumeStereo.status(), plumeStereo.selfTest().
(() => {
    if (!/(^|\.)discord\.com$/.test(location.hostname)) return;
    const cfg = window.__PLUME__ || {};
    const enabled = cfg.stereo !== false;
    const BITRATE = cfg.bitrate || 510000;
    const log = (...a) => console.log("%c[Plume stéréo]", "color:#8b7cf6;font-weight:bold", ...a);

    // ---- 2. SDP ----------------------------------------------------------
    const OPUS = { stereo: "1", "sprop-stereo": "1", maxaveragebitrate: String(BITRATE), usedtx: "0" };

    function mungeSdp(sdp) {
        if (typeof sdp !== "string") return sdp;
        const pts = [...sdp.matchAll(/a=rtpmap:(\d+) opus\/48000\/2/gi)].map(m => m[1]);
        for (const pt of pts) {
            const fmtp = new RegExp(`a=fmtp:${pt} ([^\\r\\n]*)`);
            if (fmtp.test(sdp)) {
                sdp = sdp.replace(fmtp, (_, line) => {
                    const params = new Map(line.split(";").map(p => p.trim()).filter(Boolean).map(p => {
                        const i = p.indexOf("=");
                        return i < 0 ? [p, ""] : [p.slice(0, i), p.slice(i + 1)];
                    }));
                    for (const [k, v] of Object.entries(OPUS)) params.set(k, v);
                    return `a=fmtp:${pt} ${[...params].map(([k, v]) => (v === "" ? k : `${k}=${v}`)).join(";")}`;
                });
            } else {
                const params = Object.entries(OPUS).map(([k, v]) => `${k}=${v}`).join(";");
                sdp = sdp.replace(new RegExp(`(a=rtpmap:${pt} opus/48000/2[^\\r\\n]*)(\\r?\\n)`, "i"), `$1$2a=fmtp:${pt} ${params}$2`);
            }
        }
        return sdp;
    }

    // ---- 1. Micro ----------------------------------------------------------
    const PROCESSING = ["echoCancellation", "noiseSuppression", "autoGainControl"];

    function stereoAudio(audio) {
        const a = typeof audio === "object" && audio ? { ...audio } : {};
        // Anciennes contraintes Chrome (googEchoCancellation…) : on les retire.
        for (const k of Object.keys(a)) if (k.startsWith("goog")) delete a[k];
        if (a.mandatory) a.mandatory = Object.fromEntries(Object.entries(a.mandatory).filter(([k]) => !k.startsWith("goog")));
        if (Array.isArray(a.optional)) a.optional = a.optional.filter(o => !Object.keys(o).some(k => k.startsWith("goog")));
        if (Array.isArray(a.advanced)) a.advanced = a.advanced.map(o => Object.fromEntries(Object.entries(o).filter(([k]) => !PROCESSING.includes(k) && k !== "channelCount")));
        a.channelCount = { ideal: 2 };
        for (const k of PROCESSING) a[k] = false;
        return a;
    }

    const peers = new Set();

    if (enabled) {
        const md = navigator.mediaDevices;
        const getUserMedia = md.getUserMedia.bind(md);
        md.getUserMedia = function (constraints) {
            if (constraints?.audio) constraints = { ...constraints, audio: stereoAudio(constraints.audio) };
            return getUserMedia(constraints);
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

        // ---- 3. Débit côté émetteur -------------------------------------------
        const setParameters = RTCRtpSender.prototype.setParameters;
        RTCRtpSender.prototype.setParameters = function (params, ...rest) {
            if (this.track?.kind === "audio" && params?.encodings?.length) {
                for (const e of params.encodings) e.maxBitrate = Math.max(e.maxBitrate || 0, BITRATE);
            }
            return setParameters.call(this, params, ...rest);
        };

        log(`actif, ${BITRATE / 1000} kb/s`);
    }

    // ---- Diagnostic --------------------------------------------------------
    async function status() {
        const out = [];
        for (const pc of peers) {
            if (pc.connectionState === "closed") { peers.delete(pc); continue; }
            const stats = await pc.getStats();
            const codecs = {};
            stats.forEach(s => s.type === "codec" && (codecs[s.id] = s));
            for (const sender of pc.getSenders()) {
                if (sender.track?.kind !== "audio") continue;
                const s = sender.track.getSettings();
                let codec, bitrate;
                stats.forEach(r => {
                    if (r.type === "outbound-rtp" && r.kind === "audio") {
                        codec = codecs[r.codecId];
                        bitrate = r.targetBitrate;
                    }
                });
                out.push({
                    canauxMicro: s.channelCount,
                    echoCancellation: s.echoCancellation,
                    noiseSuppression: s.noiseSuppression,
                    autoGainControl: s.autoGainControl,
                    codec: codec && `${codec.mimeType} ${codec.channels}ch`,
                    fmtp: codec?.sdpFmtpLine,
                    debitCible: bitrate,
                    stereo: s.channelCount === 2 && /stereo=1/.test(codec?.sdpFmtpLine || ""),
                });
            }
        }
        console.table(out);
        return out;
    }

    // Boucle locale : un son à gauche seulement, envoyé d'une connexion WebRTC
    // à une autre à travers le même patch. Si la droite reste muette à
    // l'arrivée, l'encodeur et le décodeur sont bien en stéréo.
    async function selfTest(seconds = 3) {
        const ctx = new AudioContext({ sampleRate: 48000 });
        const osc = ctx.createOscillator();
        const merger = ctx.createChannelMerger(2);
        osc.connect(merger, 0, 0);
        const dest = ctx.createMediaStreamDestination();
        dest.channelCount = 2;
        merger.connect(dest);
        osc.start();

        const a = new RTCPeerConnection(), b = new RTCPeerConnection();
        a.onicecandidate = e => e.candidate && b.addIceCandidate(e.candidate);
        b.onicecandidate = e => e.candidate && a.addIceCandidate(e.candidate);
        const remote = new Promise(r => (b.ontrack = e => r(e.streams[0] || new MediaStream([e.track]))));
        a.addTrack(dest.stream.getAudioTracks()[0], dest.stream);
        await a.setLocalDescription(await a.createOffer());
        await b.setRemoteDescription(a.localDescription);
        await b.setLocalDescription(await b.createAnswer());
        await a.setRemoteDescription(b.localDescription);

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
        const end = performance.now() + seconds * 1000;
        while (performance.now() < end) {
            await new Promise(r => setTimeout(r, 50));
            analysers.forEach((an, i) => {
                an.getFloatTimeDomainData(buf);
                rms[i] += Math.sqrt(buf.reduce((s, v) => s + v * v, 0) / buf.length);
            });
            n++;
        }
        const [left, right] = rms.map(v => v / n);
        const stats = await b.getStats();
        let fmtp;
        stats.forEach(s => s.type === "codec" && /opus/i.test(s.mimeType) && (fmtp = s.sdpFmtpLine));
        a.close(); b.close(); osc.stop(); ctx.close(); el.srcObject = null;
        const result = { gauche: +left.toFixed(4), droite: +right.toFixed(4), fmtp, stereo: left > 0.05 && right < left * 0.1 };
        log("auto-test", result);
        return result;
    }

    window.plumeStereo = { enabled, bitrate: BITRATE, mungeSdp, status, selfTest };
})();
