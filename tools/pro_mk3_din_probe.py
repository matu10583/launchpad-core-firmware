#!/usr/bin/env python3
"""Compare Pro Mk3 GPIO edge snapshots while sending CC64 over an external DIN adapter.

Requires: python -m pip install mido python-rtmidi
Never flashes firmware. Port names must be supplied explicitly; no MIDI loopback.
"""

import argparse
import sys
import threading
import time

REQUEST = (0x7D, 0x43, 0x46, 0x57, 0x0E, 0x01, 0x30)
RESPONSE = (0x7D, 0x43, 0x46, 0x57, 0x0E, 0x01, 0x70, 0x01, 0x28)


def snapshot(mido, pro_input, pro_output):
    for _ in pro_input.iter_pending():
        pass
    pro_output.send(mido.Message("sysex", data=REQUEST))
    deadline = time.monotonic() + 3
    while time.monotonic() < deadline:
        for msg in pro_input.iter_pending():
            if msg.type != "sysex" or tuple(msg.data[: len(RESPONSE)]) != RESPONSE:
                continue
            if len(msg.data) != len(RESPONSE) + 6 * 16:
                raise RuntimeError(f"Unexpected probe reply length: {len(msg.data)}")
            print("reply:", "F0 " + " ".join(f"{b:02X}" for b in msg.data) + " F7")
            return list(msg.data[len(RESPONSE) :])
        time.sleep(0.005)
    raise RuntimeError("No diagnostic response. Check the Pro Mk3 MIDI (not DAW) USB port and the firmware build.")


def stream_cc64(mido, output, channel, stop):
    value = 127
    while not stop.is_set():
        output.send(mido.Message("control_change", channel=channel, control=64, value=value))
        value = 0 if value else 127
        stop.wait(0.01)
    output.send(mido.Message("control_change", channel=channel, control=64, value=0))


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--list", action="store_true", help="list available MIDI input/output port names")
    parser.add_argument("--pro-in", help="Pro Mk3 USB MIDI input (for replies)")
    parser.add_argument("--pro-out", help="Pro Mk3 USB MIDI output (for the diagnostic request)")
    parser.add_argument("--din-out", help="external USB-to-DIN adapter output, connected to Pro Mk3 DIN In")
    parser.add_argument("--channel", type=int, default=1, choices=range(1, 17), metavar="1..16")
    args = parser.parse_args()
    try:
        import mido
    except ImportError as exc:
        parser.error("install mido and python-rtmidi (python -m pip install mido python-rtmidi)")
        raise exc
    if args.list:
        print("Inputs:\n " + "\n ".join(mido.get_input_names()))
        print("Outputs:\n " + "\n ".join(mido.get_output_names()))
        return
    if not all((args.pro_in, args.pro_out, args.din_out)):
        parser.error("--pro-in, --pro-out and --din-out are required unless --list is used")
    if args.din_out == args.pro_out:
        parser.error("--din-out must be an external MIDI interface, never Pro Mk3 USB output")

    with mido.open_input(args.pro_in) as pro_input, mido.open_output(args.pro_out) as pro_output, mido.open_output(args.din_out) as din_output:
        print("Baseline: no MIDI sent to DIN In")
        baseline = snapshot(mido, pro_input, pro_output)
        stop = threading.Event()
        sender = threading.Thread(target=stream_cc64, args=(mido, din_output, args.channel - 1, stop))
        print("Sending CC64 127/0 repeatedly to DIN In; recording 3 snapshots")
        sender.start()
        try:
            time.sleep(0.1)
            active = [snapshot(mido, pro_input, pro_output) for _ in range(3)]
        finally:
            stop.set()
            sender.join()
        deltas = []
        for index, count in enumerate(baseline):
            observed = max(frame[index] for frame in active)
            if observed > count:
                deltas.append((observed - count, chr(ord("A") + index // 16), index % 16, count, observed))
        for delta, port, pin, idle, observed in sorted(deltas, reverse=True)[:20]:
            print(f"GPIO{port}{pin}: baseline {idle}, active max {observed}, +{delta}")
        if not deltas:
            print("No GPIO edge increase. This does NOT rule out a DIN input on an unobserved/analog/M0 pin.")


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, OSError) as exc:
        sys.exit(str(exc))
