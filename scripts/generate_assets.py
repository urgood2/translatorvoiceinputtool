#!/usr/bin/env python3
"""
Generate tray icons and audio cues for OpenVoicy.

This script creates:
- 6 tray icon states at multiple resolutions
- 3 audio cue sounds (start, stop, error)

Run from project root:
    python scripts/generate_assets.py
"""

import math
import struct
import wave
from pathlib import Path

from PIL import Image, ImageDraw


# Directories
SCRIPT_DIR = Path(__file__).parent
PROJECT_ROOT = SCRIPT_DIR.parent
ICONS_DIR = PROJECT_ROOT / "src-tauri" / "icons"
SOUNDS_DIR = PROJECT_ROOT / "src-tauri" / "sounds"


# Icon colors (RGBA)
COLORS = {
    "idle": (46, 204, 113, 255),        # Green - ready
    "recording": (231, 76, 60, 255),     # Red - recording
    "transcribing": (241, 196, 15, 255), # Yellow - processing
    "loading": (52, 152, 219, 255),      # Blue - loading
    "error": (231, 76, 60, 255),         # Red - error
    "disabled": (149, 165, 166, 255),    # Gray - paused
}

# Audio settings
SAMPLE_RATE = 44100
DURATION_SHORT = 0.1  # 100ms
DURATION_LONG = 0.2   # 200ms


def create_icon(size: int, state: str) -> Image.Image:
    """Create a single tray icon for the given state."""
    img = Image.new("RGBA", (size, size), (0, 0, 0, 0))
    draw = ImageDraw.Draw(img)

    color = COLORS[state]
    margin = max(1, size // 8)

    if state == "idle":
        # Green circle with checkmark
        draw.ellipse(
            [margin, margin, size - margin, size - margin],
            fill=color
        )
        # Draw checkmark
        line_width = max(1, size // 8)
        cx, cy = size // 2, size // 2
        # Checkmark path
        check_points = [
            (cx - size // 4, cy),
            (cx - size // 10, cy + size // 5),
            (cx + size // 4, cy - size // 5),
        ]
        draw.line(check_points[:2], fill=(255, 255, 255, 255), width=line_width)
        draw.line(check_points[1:], fill=(255, 255, 255, 255), width=line_width)

    elif state == "recording":
        # Red solid circle (recording indicator)
        draw.ellipse(
            [margin, margin, size - margin, size - margin],
            fill=color
        )

    elif state == "transcribing":
        # Yellow circle with three dots (processing)
        draw.ellipse(
            [margin, margin, size - margin, size - margin],
            fill=color
        )
        # Draw dots
        dot_radius = max(1, size // 10)
        cy = size // 2
        for dx in [-size // 5, 0, size // 5]:
            cx = size // 2 + dx
            draw.ellipse(
                [cx - dot_radius, cy - dot_radius, cx + dot_radius, cy + dot_radius],
                fill=(255, 255, 255, 255)
            )

    elif state == "loading":
        # Blue circle with arc (loading indicator)
        draw.ellipse(
            [margin, margin, size - margin, size - margin],
            fill=color
        )
        # Draw partial ring to indicate loading
        ring_margin = margin + max(2, size // 6)
        draw.arc(
            [ring_margin, ring_margin, size - ring_margin, size - ring_margin],
            start=0, end=270,
            fill=(255, 255, 255, 255),
            width=max(1, size // 8)
        )

    elif state == "error":
        # Red circle with exclamation mark
        draw.ellipse(
            [margin, margin, size - margin, size - margin],
            fill=color
        )
        # Draw exclamation mark
        line_width = max(1, size // 6)
        cx = size // 2
        # Vertical line
        draw.line(
            [(cx, margin + size // 5), (cx, size // 2 + size // 10)],
            fill=(255, 255, 255, 255),
            width=line_width
        )
        # Dot
        dot_y = size - margin - size // 5
        dot_radius = max(1, size // 10)
        draw.ellipse(
            [cx - dot_radius, dot_y - dot_radius, cx + dot_radius, dot_y + dot_radius],
            fill=(255, 255, 255, 255)
        )

    elif state == "disabled":
        # Gray circle with pause bars
        draw.ellipse(
            [margin, margin, size - margin, size - margin],
            fill=color
        )
        # Draw pause symbol (two vertical bars)
        bar_width = max(1, size // 8)
        bar_height = size // 3
        cy = size // 2
        for dx in [-size // 8, size // 8]:
            cx = size // 2 + dx
            draw.rectangle(
                [cx - bar_width // 2, cy - bar_height // 2,
                 cx + bar_width // 2, cy + bar_height // 2],
                fill=(255, 255, 255, 255)
            )

    return img


def generate_icons():
    """Generate all tray icons at multiple resolutions."""
    ICONS_DIR.mkdir(parents=True, exist_ok=True)

    states = ["idle", "recording", "transcribing", "loading", "error", "disabled"]
    sizes = [16, 22, 32]

    for state in states:
        for size in sizes:
            # Standard resolution
            icon = create_icon(size, state)
            filename = f"tray-{state}-{size}x{size}.png"
            icon.save(ICONS_DIR / filename)
            print(f"Created {filename}")

            # @2x resolution for HiDPI
            icon_2x = create_icon(size * 2, state)
            filename_2x = f"tray-{state}-{size}x{size}@2x.png"
            icon_2x.save(ICONS_DIR / filename_2x)
            print(f"Created {filename_2x}")

    # Also create standard size without dimension suffix for easier loading
    for state in states:
        icon = create_icon(32, state)  # 32x32 as default
        filename = f"tray-{state}.png"
        icon.save(ICONS_DIR / filename)
        print(f"Created {filename}")


def generate_sine_wave(frequency: float, duration: float, amplitude: float = 0.5) -> bytes:
    """Generate a sine wave audio sample."""
    num_samples = int(SAMPLE_RATE * duration)
    samples = []

    for i in range(num_samples):
        t = i / SAMPLE_RATE
        # Apply envelope (fade in/out) to avoid clicks
        envelope = 1.0
        fade_samples = int(SAMPLE_RATE * 0.01)  # 10ms fade
        if i < fade_samples:
            envelope = i / fade_samples
        elif i > num_samples - fade_samples:
            envelope = (num_samples - i) / fade_samples

        value = amplitude * envelope * math.sin(2 * math.pi * frequency * t)
        # Convert to 16-bit signed integer
        sample = int(value * 32767)
        samples.append(struct.pack('<h', sample))

    return b''.join(samples)


def write_wav(filename: Path, audio_data: bytes):
    """Write audio data to a WAV file."""
    with wave.open(str(filename), 'wb') as wav:
        wav.setnchannels(1)  # Mono
        wav.setsampwidth(2)  # 16-bit
        wav.setframerate(SAMPLE_RATE)
        wav.writeframes(audio_data)


def generate_sounds():
    """Generate audio cue sounds."""
    SOUNDS_DIR.mkdir(parents=True, exist_ok=True)

    # Start sound: Rising tone (pleasant, confirming)
    # Two quick notes going up
    start_tone1 = generate_sine_wave(880, DURATION_SHORT / 2, 0.3)  # A5
    start_tone2 = generate_sine_wave(1046.5, DURATION_SHORT / 2, 0.3)  # C6
    start_audio = start_tone1 + start_tone2
    write_wav(SOUNDS_DIR / "cue-start.wav", start_audio)
    print("Created cue-start.wav")

    # Stop sound: Falling tone (confirmation)
    # Two quick notes going down
    stop_tone1 = generate_sine_wave(1046.5, DURATION_SHORT / 2, 0.3)  # C6
    stop_tone2 = generate_sine_wave(880, DURATION_SHORT / 2, 0.3)  # A5
    stop_audio = stop_tone1 + stop_tone2
    write_wav(SOUNDS_DIR / "cue-stop.wav", stop_audio)
    print("Created cue-stop.wav")

    # Error sound: Dissonant/warning tone
    # Lower frequency, slightly longer
    error_audio = generate_sine_wave(330, DURATION_LONG, 0.4)  # E4 - warning tone
    write_wav(SOUNDS_DIR / "cue-error.wav", error_audio)
    print("Created cue-error.wav")


def main():
    """Generate all assets."""
    print("Generating tray icons...")
    generate_icons()

    print("\nGenerating audio cues...")
    generate_sounds()

    print("\nAsset generation complete!")
    print(f"Icons saved to: {ICONS_DIR}")
    print(f"Sounds saved to: {SOUNDS_DIR}")


if __name__ == "__main__":
    main()
