import sys, subprocess, time; sys.path.insert(0, ".")
from scenes import *
out = sys.argv[1] if len(sys.argv) > 1 else f"dito-intro-{LANG}-preview.mp4"
n = int(round(DURATION * FPS))
VF = "scale=out_color_matrix=bt709:out_range=tv:flags=accurate_rnd+full_chroma_int+area"
COLOR = ["-colorspace", "bt709", "-color_primaries", "bt709", "-color_trc", "bt709", "-color_range", "tv"]
APP_VF = "scale=out_color_matrix=bt601:out_range=pc:flags=accurate_rnd+full_chroma_int+error_diffusion"
RAW = ["ffmpeg", "-y", "-loglevel", "error", "-f", "rawvideo", "-pix_fmt", "rgb24", "-s", f"{W}x{H}", "-r", str(FPS), "-i", "-"]
app = subprocess.Popen(RAW + ["-vf", APP_VF + ",format=yuv420p", "-c:v", "libx265", "-preset", "slow", "-crf", "16", "-tag:v", "hvc1", "-x265-params", "aq-mode=3:psy-rd=1.0:log-level=error", "-colorspace", "smpte170m", "-color_primaries", "smpte170m", "-color_trc", "smpte170m", "-color_range", "pc", "-movflags", "+faststart", out.replace(".mp4", "-app.mp4")], stdin=subprocess.PIPE)
hevc = subprocess.Popen(RAW + ["-vf", VF + ",format=yuv420p10le", "-c:v", "libx265", "-preset", "medium", "-crf", "15", "-tag:v", "hvc1",
    "-x265-params", "aq-mode=3:psy-rd=1.0:log-level=error", *COLOR, "-movflags", "+faststart", out], stdin=subprocess.PIPE)
h264 = subprocess.Popen(RAW + ["-vf", VF + ",format=yuv420p", "-c:v", "libx264", "-preset", "slow", "-crf", "14", "-tune", "grain",
    "-x264-params", "aq-mode=3", *COLOR, "-movflags", "+faststart", out.replace(".mp4", "-h264.mp4")], stdin=subprocess.PIPE)
t0 = time.time()
for i in range(n):
    fb = render(i / FPS).tobytes(); hevc.stdin.write(fb); h264.stdin.write(fb); app.stdin.write(fb)
    if i % 150 == 0: print(i, n, round(time.time() - t0), "s", flush=True)
hevc.stdin.close(); h264.stdin.close(); app.stdin.close(); hevc.wait(); h264.wait(); app.wait(); print("done", out, round(time.time() - t0), "s")
