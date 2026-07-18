import 'package:flutter/material.dart';

void main() {
  runApp(const EduHubApp());
}

class EduHubApp extends StatelessWidget {
  const EduHubApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp(
      title: 'EduHub179 (scaffold)',
      home: Scaffold(
        appBar: AppBar(title: const Text('EduHub179')),
        body: const Center(child: Text('Mobile scaffold: run `flutter run` in mobile/`')),
      ),
    );
  }
}
