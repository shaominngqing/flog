abstract class FlogTimingClock {
  int nowUs();
}

class StopwatchTimingClock implements FlogTimingClock {
  final Stopwatch _stopwatch;

  StopwatchTimingClock() : _stopwatch = Stopwatch()..start();

  @override
  int nowUs() => _stopwatch.elapsedMicroseconds;
}

class ManualTimingClock implements FlogTimingClock {
  int valueUs;

  ManualTimingClock([this.valueUs = 0]);

  @override
  int nowUs() => valueUs;

  void advanceUs(int deltaUs) {
    valueUs += deltaUs;
  }
}
