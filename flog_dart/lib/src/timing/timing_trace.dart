class FlogTimingConnection {
  final String? id;
  final bool reused;
  final String? protocol;
  final String? proxy;

  const FlogTimingConnection({
    this.id,
    this.reused = false,
    this.protocol,
    this.proxy,
  });

  Map<String, dynamic> toJson() => <String, dynamic>{
        if (id != null) 'id': id,
        'reused': reused,
        if (protocol != null) 'protocol': protocol,
        if (proxy != null) 'proxy': proxy,
      };
}

class FlogTimingPhase {
  final String name;
  final int? startUs;
  final int? endUs;
  final String status;
  final String confidence;
  final String? detail;

  const FlogTimingPhase({
    required this.name,
    this.startUs,
    this.endUs,
    this.status = 'complete',
    this.confidence = 'exact',
    this.detail,
  });

  int? get durationUs =>
      startUs == null || endUs == null ? null : endUs! - startUs!;

  Map<String, dynamic> toJson() => <String, dynamic>{
        'name': name,
        if (startUs != null) 'startUs': startUs,
        if (endUs != null) 'endUs': endUs,
        'status': status,
        'confidence': confidence,
        if (detail != null) 'detail': detail,
      };
}

class FlogTimingEvent {
  final String name;
  final int? atUs;
  final int? gapUs;
  final int? size;
  final String? detail;

  const FlogTimingEvent({
    required this.name,
    this.atUs,
    this.gapUs,
    this.size,
    this.detail,
  });

  Map<String, dynamic> toJson() => <String, dynamic>{
        'name': name,
        if (atUs != null) 'atUs': atUs,
        if (gapUs != null) 'gapUs': gapUs,
        if (size != null) 'size': size,
        if (detail != null) 'detail': detail,
      };
}

class FlogTimingTrace {
  final int version;
  final String source;
  final String clock;
  final int startUs;
  final int? endUs;
  final FlogTimingConnection? connection;
  final List<FlogTimingPhase> phases;
  final List<FlogTimingEvent> events;

  const FlogTimingTrace({
    this.version = 1,
    required this.source,
    this.clock = 'monotonic_us',
    required this.startUs,
    this.endUs,
    this.connection,
    required this.phases,
    required this.events,
  });

  int? get durationUs => endUs == null ? null : endUs! - startUs;

  FlogTimingTrace finish(int endUs) => FlogTimingTrace(
        version: version,
        source: source,
        clock: clock,
        startUs: startUs,
        endUs: endUs,
        connection: connection,
        phases: phases,
        events: events,
      );

  FlogTimingTrace copyWith({
    int? endUs,
    FlogTimingConnection? connection,
    List<FlogTimingPhase>? phases,
    List<FlogTimingEvent>? events,
  }) =>
      FlogTimingTrace(
        version: version,
        source: source,
        clock: clock,
        startUs: startUs,
        endUs: endUs ?? this.endUs,
        connection: connection ?? this.connection,
        phases: phases ?? this.phases,
        events: events ?? this.events,
      );

  Map<String, dynamic> toJson() => <String, dynamic>{
        'v': version,
        'source': source,
        'clock': clock,
        'startUs': startUs,
        if (endUs != null) 'endUs': endUs,
        if (connection != null) 'connection': connection!.toJson(),
        'phases': phases.map((phase) => phase.toJson()).toList(growable: false),
        'events': events.map((event) => event.toJson()).toList(growable: false),
      };
}
