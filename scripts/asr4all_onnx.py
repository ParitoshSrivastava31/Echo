
"""asr4all ONNX example: transcribe with a release `onnx/` directory.

pip install onnxruntime numpy

python asr4all_onnx.py onnx/ clip.wav           # punctuated + cased
python asr4all_onnx.py onnx/ clip.wav --tier 2  # a different latency point
python asr4all_onnx.py onnx/ clip.wav --stream   # chunk by chunk
python asr4all_onnx.py onnx/ --list-tiers

PCEC is the model's second pass over the recognised tokens: punctuation, capitalisation and
error correction. It runs a little behind the acoustic transcript; `--no-pcec` skips it.
"""

from __future__ import annotations

import argparse
import json
import sys
import wave
from pathlib import Path

import numpy as np
import onnxruntime as ort

SAMPLE_RATE = 16_000
# Component I/O, named per tier:
#   in  {tier}_pcm,          filter_cache,             attn_cache
#   out {tier}_log_probs,    {tier}_filter_cache_out,  {tier}_attn_cache_out,
#       {tier}_hidden (only with PCEC)
# The caches are not all float: the PCEC `seen` counter is int64. Zero-fill each one with
# the dtype the graph declares rather than assuming float32.
NP_DTYPE = {"tensor(int64)": np.int64, "tensor(float)": np.float32,
            "tensor(float16)": np.float16}  # fmt: skip
# Blank-frame counts separating the pause buckets PCEC was trained on.
PAUSE_THRESHOLDS = (1, 2, 4, 8, 16, 32, 64, 128)


def load_wav(path: str) -> np.ndarray:
    """16 kHz mono 16-bit .wav -> float32 in [-1, 1]."""
    with wave.open(path) as w:
        if w.getframerate() != SAMPLE_RATE or w.getnchannels() != 1:
            raise ValueError(f"{path}: want {SAMPLE_RATE} Hz mono")
        buf = np.frombuffer(w.readframes(w.getnframes()), dtype=np.int16)
    return buf.astype(np.float32) / 32768.0


def _pause_bucket(blanks: int) -> int:
    return sum(1 for t in PAUSE_THRESHOLDS if blanks >= t)


class _Pcec:
    """
    The token-space second pass for punctuation, capitalization and error correction
    """

    def __init__(self, meta: dict, rung: dict, model, hidden_dim: int):
        self.meta, self.model = meta, model
        self.name, self.flush_name = rung["name"], rung["flush"]
        self.n_cache = int(rung.get("cache_tensors", 2))
        p = self.name
        self.commit = int(model.shape(f"{p}_hyp_ids")[1])
        flush_slots = int(model.out_shape(f"{p}_tail_slots")[1])
        # the model's own settle lag in tokens, read off the flush width
        self.lookahead = (flush_slots - 1) // 2 - self.commit
        self.wants_acoustic = f"{p}_acoustic" in model.inputs
        self.wants_keep = f"{p}_keep" in model.inputs
        self.dim = hidden_dim
        self._cache_spec = [(f"{p}_state", None), (f"{p}_seen", None)]
        self.reset()

    @property
    def lag(self) -> tuple[int, int, int]:
        """(commit, lookahead, settle) in tokens, ie how far decode_pcec trails the audio."""
        return (self.commit, self.lookahead, self.lookahead + self.commit - 1)

    def reset(self) -> None:
        self.cache = [self.model.zeros(n) for n, _ in self._cache_spec]
        self.ids: list[int] = []
        self.pause: list[int] = []
        self.acoustic: list[np.ndarray] = []
        self.keep: list[float] = []  # per token, 1.0 = written by the word spotter
        self.slots: list[int] = []
        self.consumed = 0
        self._open = None  # [id, hidden sum, frames, leading blanks]
        self._blanks = 0
        self._prev = -1

    def pool(self, frame_ids: list[int], hidden: np.ndarray, blank: int, keep=None) -> None:
        """One committed chunk of frames -> pooled tokens. `keep` (per frame, from a biasing
        decoder) pools to a per-token max: a token any of whose frames is marked is kept."""
        for t, fid in enumerate(frame_ids):
            kf = float(keep[t]) if keep is not None else 0.0
            if fid != self._prev and fid != blank:  # a new token starts here
                if self._open:
                    self._emit()
                self._open = [fid, hidden[t].astype(np.float64).copy(), 1, self._blanks, kf]
                self._blanks = 0
            elif self._open:  # a repeat or a blank belongs to the open token
                self._open[1] += hidden[t]
                self._open[2] += 1
                self._open[4] = max(self._open[4], kf)
            self._prev = fid
            if fid == blank:
                self._blanks += 1

    def flush(self) -> None:
        """Close the token still held back."""
        if self._open:
            self._emit()

    def _emit(self) -> None:
        tok, acc, n, run, kf = self._open
        self._open = None
        self.ids.append(tok)
        self.acoustic.append((acc / n).astype(np.float32))
        self.pause.append(_pause_bucket(run))
        self.keep.append(kf)

    def drain(self) -> None:
        """Run every whole commit that is ready; leftovers stay buffered."""
        k = self.commit
        while len(self.ids) >= k:
            outs = self.model.run_pcec(self, self._feats(k, k), self.cache)
            # The delay line holds `lookahead` tokens; until that many have gone through it,
            # its head is still warm-up from before the utterance. Drop those slots.
            skip = min(2 * k, max(0, 2 * self.lookahead - 2 * self.consumed))
            self.slots += outs[0][0].tolist()[skip : 2 * k]
            self.cache = list(outs[-self.n_cache :])
            del self.ids[:k], self.pause[:k], self.acoustic[:k], self.keep[:k]
            self.consumed += k

    def tail(self) -> list[int]:
        """The last partial commit plus whatever is still inside the model."""
        n = len(self.ids)
        slots = self.model.run_pcec_flush(self, self._feats(self.commit, n), n, self.cache)
        valid = 2 * self.lookahead + 2 * n + 1
        skip = max(0, 2 * self.lookahead - 2 * self.consumed)
        return slots[0].tolist()[skip:valid]

    def _feats(self, k: int, n: int) -> list[np.ndarray]:
        """A full block of `k` tokens, padded past the `n` real ones."""
        ids = np.full((1, k), int(self.meta["pad_in"]), dtype=np.int64)
        pause = np.full((1, k), int(self.meta["pause_blackout"]), dtype=np.int64)
        for i in range(n):
            ids[0, i] = self.ids[i]
            # real_pause is a training decision, invisible in the graph
            # a model trained with the channel blacked out must be fed the blackout value
            if self.meta.get("real_pause"):
                pause[0, i] = self.pause[i]
        feats = [ids, pause]
        if self.wants_acoustic:
            ac = np.zeros((1, k, self.dim), dtype=np.float32)
            for i in range(n):
                ac[0, i] = self.acoustic[i]
            feats.append(ac)
        if self.wants_keep:
            # `keep`: per input token, how strongly PCEC should preserve it (0..1). A decoder that
            # biases recognition toward user vocabulary sets 1 on the words it inserted so PCEC
            # does not revert them. With no such decoder every value is 0 and PCEC runs as it
            # always did, which is what this script does unless set_dictionary() is used.
            kp = np.zeros((1, k), dtype=np.float32)
            for i in range(n):
                kp[0, i] = self.keep[i]
            feats.append(kp)
        return feats

    def render(self, slots) -> str:
        return render_slots(self.meta, slots)


def render_slots(meta: dict, slots) -> str:
    """
    Lattice slots -> punctuated, cased text.
    """
    vocab, blank, ctrl = meta["vocab"], meta["blank_id"], meta["ctrl"]
    case = {ctrl["cap"]: "cap", ctrl["allcaps"]: "upper"}
    punct = {ctrl["period"]: ".", ctrl["comma"]: ",", ctrl["question"]: "?"}
    words: list[list] = []  # [pieces, case, punct]
    pending, prev = "", -1
    for i in slots:
        if i in (prev, blank):  # CTC collapse
            prev = i
            continue
        prev = i
        if i in case:
            pending = case[i]
        elif i in punct:
            if words:
                words[-1][2] = punct[i]
        elif i < len(vocab) and vocab[i]:
            if vocab[i].startswith(" ") or not words:
                words.append([[vocab[i]], pending, ""])
                pending = ""
            else:
                words[-1][0].append(vocab[i])
    out, sentence_start = [], True
    for pieces, cas, pnc in words:
        w = "".join(pieces).strip()
        if not w:
            continue
        if cas == "upper":
            w = w.upper()
        elif cas == "cap" or sentence_start:
            w = w[:1].upper() + w[1:]
        sentence_start = pnc in (".", "?")
        out.append(w + pnc)
    return " ".join(out)


# ONNX TensorProto data types -> numpy
_NP_OF_ONNX = {1: np.float32, 6: np.int32, 7: np.int64, 9: np.bool_, 10: np.float16, 11: np.float64}


class _Model:
    """The release's ONNX components: one graph per tier, per PCEC rung and flush, and per head,
    all naming their weights in ONE shared file (`asr4all.onnx.data`).

    Every component is loaded up front, so switching tiers is instant, and the weights are held
    once: the data file is memory-mapped and every session is handed the same tensors. Two ONNX
    Runtime defaults would undo that sharing and are turned off here: weight prepacking (a
    per-session copy of every Gemm/Conv weight) and the CPU memory arena (a per-session
    preallocation). Neither changes speed on these graphs."""

    def __init__(self, onnx_dir: Path, meta: dict):
        available_graphs = {k: v for k, v in meta["graphs"].items() if (onnx_dir / v).is_file()}
        weights = (
            self._map_weights(onnx_dir / meta["weights"], available_graphs)
            if meta.get("weights")
            else {}
        )
        self._s: dict[str, ort.InferenceSession] = {}
        self.inputs: dict[str, tuple] = {}
        self.outputs: dict[str, tuple] = {}
        for name, file in available_graphs.items():
            so = ort.SessionOptions()
            so.log_severity_level = 3  # errors only
            so.enable_cpu_mem_arena = False
            so.add_session_config_entry("session.disable_prepacking", "1")
            for tensor, value in weights.get(file, {}).items():
                so.add_initializer(tensor, value)
            sess = ort.InferenceSession(
                str(onnx_dir / file), so, providers=["CPUExecutionProvider"]
            )
            self._s[name] = sess
            self.inputs.update({i.name: (i.shape, i.type) for i in sess.get_inputs()})
            self.outputs.update({o.name: (o.shape, o.type) for o in sess.get_outputs()})

    @classmethod
    def _map_weights(cls, data_file: Path, graph_files: dict) -> dict[str, dict[str, ort.OrtValue]]:
        """{graph file: {tensor name: OrtValue}}, every OrtValue a view on one memory map of the
        data file, so each weight is resident once however many components use it. The offsets
        come from each graph's own external-data records; reading those needs no `onnx` package,
        only the ONNX protobuf wire format, walked here for the few fields it needs."""
        data = np.memmap(data_file, dtype=np.uint8, mode="r")
        # an OrtValue built from numpy does NOT keep the array (or the map under it) alive: hold
        # both for the model's lifetime, or the first run reads unmapped memory
        cls._keepalive = getattr(cls, "_keepalive", [])
        cls._keepalive.append(data)
        shared: dict[tuple, ort.OrtValue] = {}
        out: dict[str, dict[str, ort.OrtValue]] = {}
        for file in graph_files.values():
            refs = {}
            for t in _external_tensors((data_file.parent / file).read_bytes()):
                key = (t["offset"], t["length"])
                if key not in shared:
                    arr = data[t["offset"] : t["offset"] + t["length"]].view(
                        _NP_OF_ONNX[t["dtype"]]
                    )
                    arr = arr.reshape(t["dims"])
                    cls._keepalive.append(arr)
                    shared[key] = ort.OrtValue.ortvalue_from_numpy(arr)
                refs[t["name"]] = shared[key]
            out[file] = refs
        return out

    def shape(self, name):
        return self.inputs[name][0]

    def out_shape(self, name):
        return self.outputs[name][0]

    def zeros(self, name):
        shape, typ = self.inputs[name]
        return np.zeros(tuple(int(d) for d in shape), dtype=NP_DTYPE[typ])

    def run_stream(self, tier, win, filter_cache, attn_cache):
        n = tier["name"]
        outs = [f"{n}_log_probs", f"{n}_filter_cache_out", f"{n}_attn_cache_out"]
        if tier["n_out"] > 3:
            outs.append(f"{n}_hidden")
        return self._s[n].run(
            outs,
            {f"{n}_pcm": win[None, :], "filter_cache": filter_cache, "attn_cache": attn_cache},
        )

    def _feed(self, pc, feats, cache):
        p = pc.name
        feed = {f"{p}_hyp_ids": feats[0], f"{p}_pause": feats[1]}
        if pc.wants_acoustic:
            feed[f"{p}_acoustic"] = feats[2]
        if pc.wants_keep:
            feed[f"{p}_keep"] = feats[3]
        feed[f"{p}_state"], feed[f"{p}_seen"] = cache[0], cache[1]
        return feed

    def run_pcec(self, pc, feats, cache):
        p = pc.name
        return self._s[p].run(
            [f"{p}_slots", f"{p}_state_out", f"{p}_seen_out"], self._feed(pc, feats, cache)
        )

    def run_pcec_flush(self, pc, feats, n_real, cache):
        p = pc.name
        feed = self._feed(pc, feats, cache)
        feed[f"{p}_n_real"] = np.array(n_real, dtype=np.int64)
        (slots,) = self._s[f"{p}_flush"].run([f"{p}_tail_slots"], feed)
        return slots


def _varint(buf: bytes, i: int) -> tuple[int, int]:
    shift = val = 0
    while True:
        b = buf[i]
        i += 1
        val |= (b & 0x7F) << shift
        if not b & 0x80:
            return val, i
        shift += 7


def _fields(buf: bytes):
    """(field number, wire type, value) over one protobuf message; value is an int for varints
    and a memoryview for length-delimited fields."""
    i, mv = 0, memoryview(buf)
    while i < len(buf):
        key, i = _varint(buf, i)
        num, wt = key >> 3, key & 7
        if wt == 0:
            val, i = _varint(buf, i)
        elif wt == 2:
            n, i = _varint(buf, i)
            val, i = mv[i : i + n], i + n
        elif wt == 1:
            val, i = None, i + 8
        elif wt == 5:
            val, i = None, i + 4
        else:
            raise ValueError(f"unsupported protobuf wire type {wt}")
        yield num, wt, val


def _external_tensors(model: bytes):
    """The initializers of an ONNX model that live in external data: name, dtype, dims, offset,
    length. ModelProto.graph = 7, GraphProto.initializer = 5; TensorProto: dims = 1, data_type
    = 2, name = 8, external_data = 13 (key 1 / value 2), data_location = 14 (1 = EXTERNAL)."""
    for num, _, graph in _fields(model):
        if num != 7:
            continue
        for gnum, _, tensor in _fields(bytes(graph)):
            if gnum != 5:
                continue
            t = {"dims": [], "external": {}}
            location = 0
            for tnum, wt, val in _fields(bytes(tensor)):
                if tnum == 1:
                    if wt == 0:
                        t["dims"].append(val)
                    else:  # packed
                        j, raw = 0, bytes(val)
                        while j < len(raw):
                            d, j = _varint(raw, j)
                            t["dims"].append(d)
                elif tnum == 2:
                    t["dtype"] = val
                elif tnum == 8:
                    t["name"] = bytes(val).decode()
                elif tnum == 13:
                    kv = {k: bytes(v).decode() for k, _, v in _fields(bytes(val)) if v is not None}
                    t["external"][kv.get(1)] = kv.get(2)
                elif tnum == 14:
                    location = val
            if location == 1:
                ext = t["external"]
                yield {
                    "name": t["name"],
                    "dtype": t["dtype"],
                    "dims": t["dims"],
                    "offset": int(ext.get("offset", 0)),
                    "length": int(ext["length"]),
                }


class Engine:
    """Load a bundle, pick a tier and decode."""

    def __init__(self, onnx_dir: str):
        d = Path(onnx_dir)
        self.meta = json.loads((d / "metadata.json").read_text(encoding="utf-8"))
        self.model = _Model(d, self.meta)
        self.vocab, self.blank = self.meta["vocab"], self.meta["blank_id"]
        self.spf = int(self.meta["streaming"]["samples_per_frame"])
        # Geometry comes from the graph, not the metadata, so the ladder cannot disagree
        # with what was exported.
        rungs = (self.meta.get("pcec") or {}).get("rungs", {})
        assign = (self.meta.get("pcec") or {}).get("tier_rung", {})
        self.tiers = []
        for name in self.meta["tiers"]:
            if name not in self.model._s:
                continue
            window = int(self.model.shape(f"{name}_pcm")[-1])
            commit = int(self.model.out_shape(f"{name}_log_probs")[1])
            has_hidden = f"{name}_hidden" in self.model.outputs
            rung = rungs.get(str(assign.get(name))) or rungs.get(assign.get(name))
            has_pcec = rung and rung["name"] in self.model._s and rung["flush"] in self.model._s
            self.tiers.append({
                "name": name,
                "pcec": rung if has_pcec else None,
                "window": window,
                "commit": commit,
                "lookahead": window // self.spf - commit,
                "n_out": 4 if has_hidden else 3,
                "dim": int(self.model.out_shape(f"{name}_hidden")[-1]) if has_hidden else 0,
            })  # fmt: skip
        self.tiers.sort(key=lambda t: t["lookahead"] + t["commit"])  # the window: the dial
        self.spotter = None  # personal dictionary, see set_dictionary()
        avail_names = [t["name"] for t in self.tiers]
        default_idx = avail_names.index(self.meta["default_tier"]) if self.meta["default_tier"] in avail_names else 0
        self.set_tier(default_idx)

    def set_tier(self, idx: int | str) -> None:
        """Select a tier by index (0 = lowest latency) or by name (`stream_c256r32`)."""
        names = [t["name"] for t in self.tiers]
        if isinstance(idx, str):
            if idx.lstrip("-").isdigit():
                idx = int(idx)
            elif idx in names:
                idx = names.index(idx)
            else:
                raise ValueError(f"unknown tier {idx!r}: have {names}")
        if not 0 <= idx < len(self.tiers):
            raise IndexError(f"tier {idx}: have 0..{len(self.tiers) - 1}")
        self.idx = idx
        t = self.tiers[idx]
        self.pcec = (
            _Pcec(self.meta["pcec"], t["pcec"], self.model, t["dim"])
            if self.meta.get("pcec") and t.get("pcec")
            else None
        )
        self.reset()

    def reset(self) -> None:
        """Start a new utterance. Zero caches are the initialized state."""
        # the caches are shared graph inputs: same shape whichever tier produced them, so
        # one host state serves every tier and switching tiers needs no conversion
        self.filter_cache = self.model.zeros("filter_cache")
        self.attn_cache = self.model.zeros("attn_cache")
        self.pcm = np.zeros(0, dtype=np.float32)
        self.next_frame = 0
        self.ids: list[int] = []
        if self.pcec:
            self.pcec.reset()
        if self.spotter:
            self.spotter.reset()

    def pcec_lag(self):
        """(commit, lookahead, settle) in tokens, or None without PCEC."""
        return self.pcec.lag if self.pcec else None

    def set_dictionary(self, words, **spotter_kw) -> None:
        """Contextual biasing hook: favour user vocabulary (names, jargon) where the audio nearly
        supports it, and mark those words so PCEC preserves them. It needs a word-spotting module
        (`word_spotter.py` beside this file) that is NOT part of this release; without it this
        method raises ImportError. Pass an empty list to switch biasing off, which is the default:
        PCEC then receives keep = 0 for every token and behaves exactly as before."""
        if not words:  # biasing off: needs nothing beyond this script
            self.spotter = None
            self.reset()
            return
        # imported as a sibling file, so this script depends on nothing beyond its own directory
        _here = str(Path(__file__).resolve().parent)
        if _here not in sys.path:
            sys.path.insert(0, _here)
        from word_spotter import build_spotter

        merges = self.meta.get("merges")
        if words and not merges:
            raise ValueError(
                "this bundle's metadata carries no `merges`; re-export it to use a dictionary"
            )
        self.spotter = build_spotter(words, self.vocab, self.blank, merges or [], **spotter_kw)
        self.reset()

    def decode_stream(self, chunk: np.ndarray) -> str:
        """Feed audio, commit every complete window, return the transcript so far."""
        t = self.tiers[self.idx]
        self.pcm = np.concatenate([self.pcm, chunk])
        window_frames = t["window"] // self.spf
        while (self.next_frame + window_frames) * self.spf <= len(self.pcm):
            self._step(t)
        return self._text()

    def finish(self) -> str:
        """Flush the tail (zero-padding the missing lookahead) and close PCEC."""
        t = self.tiers[self.idx]
        while self.next_frame * self.spf < len(self.pcm):
            self._step(t)
        if self.spotter:
            self._commit(*self.spotter.flush())
        if self.pcec:
            self.pcec.flush()
            self.pcec.drain()
        return self._text()

    def decode_pcec(self, decode_tail: bool = False) -> str:
        """Punctuated + cased text. It trails the acoustic transcript by `pcec_lag()`; pass
        decode_tail=True at an endpoint to also render what is still inside the model."""
        if not self.pcec:
            return self._text()
        slots = self.pcec.slots + (self.pcec.tail() if decode_tail else [])
        return self.pcec.render(slots)

    def decode_offline(self, clip: np.ndarray, apply_pcec: bool = True) -> str:
        """A whole clip: the streaming loop run to completion."""
        self.reset()
        self.decode_stream(clip)
        self.finish()
        return self.decode_pcec(decode_tail=True) if (apply_pcec and self.pcec) else self._text()

    def _step(self, t: dict) -> None:
        """One method call: window + caches -> committed frames, updated caches."""
        start = self.next_frame * self.spf
        # Zero-pad rather than shorten: the window is a fixed shape. No front trim.
        # The window starts at the commit boundary and left context comes from the caches.
        win = np.zeros(t["window"], dtype=np.float32)
        hi = min(len(self.pcm), start + t["window"])
        if hi > start:
            win[: hi - start] = self.pcm[start:hi]
        outs = self.model.run_stream(t, win, self.filter_cache, self.attn_cache)
        self.filter_cache, self.attn_cache = outs[1], outs[2]
        valid = min(t["commit"], len(self.pcm) // self.spf - self.next_frame)
        lp = outs[0][0, :valid]
        frame_ids = lp.argmax(-1).tolist()
        hidden = outs[3][0, :valid] if (self.pcec and t["n_out"] > 3) else None
        if self.spotter:  # hold frames until no dictionary word can still rewrite them
            frame_ids, hidden, keep = self.spotter.feed(lp, frame_ids, hidden)
            self._commit(frame_ids, hidden, keep)
        else:
            self._commit(frame_ids, hidden)
        self.next_frame += t["commit"]

    def _commit(self, frame_ids: list[int], hidden, keep=None) -> None:
        """Frames that are final: into the transcript, and into PCEC (with per-frame keep marks
        when a biasing decoder supplies them)."""
        self.ids += frame_ids
        if self.pcec and hidden is not None and len(frame_ids):
            self.pcec.pool(frame_ids, hidden, self.blank, keep)
            self.pcec.drain()

    def _text(self) -> str:
        """CTC collapse of the accumulated frame ids: drop repeats, drop blanks."""
        out, prev = [], -1
        for i in self.ids:
            if i != prev and i != self.blank and i < len(self.vocab):
                out.append(self.vocab[i])
            prev = i
        return " ".join("".join(out).split())


def load_engine(onnx_dir: str) -> Engine:
    return Engine(onnx_dir)


def main() -> None:
    ap = argparse.ArgumentParser(description="asr4all ONNX example")
    ap.add_argument("onnx_dir", help="the release's onnx/ directory")
    ap.add_argument("audio", nargs="?", help="16 kHz mono wav")
    ap.add_argument(
        "--tier", default=None, help="tier name (e.g. stream_c256r32) or index (0 = lowest latency)"
    )
    ap.add_argument("--no-pcec", action="store_true", help="plain lowercase decode")
    ap.add_argument("--stream", action="store_true", help="feed in chunks, print each result")
    ap.add_argument("--list-tiers", action="store_true")
    args = ap.parse_args()

    e = load_engine(args.onnx_dir)
    if args.tier is not None:
        e.set_tier(args.tier)
    if e.pcec is not None and getattr(e.pcec, "wants_keep", False):
        print(
            "# note: this bundle's PCEC takes a `keep` input (per token, for contextual biasing); "
            "this script feeds it 0 everywhere, so PCEC behaves as it always did.",
            file=sys.stderr,
        )

    if args.list_tiers:
        print(f"{'#':>2} {'tier':16} {'commit':>7} {'lookahead':>10} {'first result':>13}  pcec")
        for i, t in enumerate(e.tiers):
            ttft = (t["commit"] + t["lookahead"]) * 40 / 1000
            print(f"{i:>2} {t['name']:16} {t['commit']:>7} {t['lookahead']:>10} "
                  f"{ttft:>10.2f} s  {(t.get('pcec') or {}).get('name', '-')}")  # fmt: skip
        return

    clip = load_wav(args.audio)
    if args.stream:
        for i in range(0, len(clip), 8000):  # 0.5 s at a time, like a capture loop
            print(e.decode_stream(clip[i : i + 8000]))
        print(e.finish())  # the tail, which never got its full lookahead
        if e.pcec and not args.no_pcec:
            print("pcec:", e.decode_pcec(decode_tail=True))
        return

    print(e.decode_offline(clip, apply_pcec=not args.no_pcec))


if __name__ == "__main__":
    main()


