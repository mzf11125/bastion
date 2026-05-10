import { useEffect, useRef } from 'react';

const VIDEO_URL =
  'https://d8j0ntlcm91z4.cloudfront.net/user_38xzZboKViGWJOttwIXH07lWA1P/hf_20260328_083109_283f3553-e28f-428b-a723-d639c617eb2b.mp4';

const FADE_DURATION = 0.5; // seconds

export function VideoBackground() {
  const videoRef = useRef<HTMLVideoElement>(null);
  const rafRef = useRef<number>(0);

  useEffect(() => {
    const video = videoRef.current;
    if (!video) return;

    function tick() {
      if (!video) return;
      const { currentTime, duration } = video;
      if (!duration) {
        rafRef.current = requestAnimationFrame(tick);
        return;
      }

      if (currentTime < FADE_DURATION) {
        // Fade in
        video.style.opacity = String(currentTime / FADE_DURATION);
      } else if (currentTime > duration - FADE_DURATION) {
        // Fade out
        video.style.opacity = String((duration - currentTime) / FADE_DURATION);
      } else {
        video.style.opacity = '1';
      }

      rafRef.current = requestAnimationFrame(tick);
    }

    function handleEnded() {
      if (!video) return;
      video.style.opacity = '0';
      setTimeout(() => {
        if (!video) return;
        video.currentTime = 0;
        video.play().catch(() => {});
      }, 100);
    }

    video.style.opacity = '0';
    video.addEventListener('ended', handleEnded);
    rafRef.current = requestAnimationFrame(tick);

    return () => {
      cancelAnimationFrame(rafRef.current);
      video.removeEventListener('ended', handleEnded);
    };
  }, []);

  return (
    <div
      className="absolute inset-x-0 bottom-0 overflow-hidden pointer-events-none"
      style={{ top: '300px', zIndex: 0 }}
      aria-hidden="true"
    >
      {/* The video */}
      <video
        ref={videoRef}
        src={VIDEO_URL}
        autoPlay
        muted
        playsInline
        className="w-full h-full object-cover"
        style={{ opacity: 0, transition: 'none' }}
      />

      {/* Contrast overlay — light mode washes white, dark mode washes black */}
      <div className="absolute inset-0 bg-white/50 dark:bg-black/60 transition-colors duration-300" />

      {/* Top gradient — fades from page background into the video */}
      <div className="absolute inset-x-0 top-0 h-48 bg-gradient-to-b from-[var(--bg)] to-transparent" />

      {/* Bottom gradient — fades from video back to page background */}
      <div className="absolute inset-x-0 bottom-0 h-48 bg-gradient-to-t from-[var(--bg)] to-transparent" />
    </div>
  );
}
