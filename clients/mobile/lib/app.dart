import 'package:duskcue_mobile/navigation/app_router.dart';
import 'package:flutter/material.dart';

class DuskcueApp extends StatelessWidget {
  const DuskcueApp({super.key});

  @override
  Widget build(BuildContext context) {
    return MaterialApp.router(
      title: 'Duskcue',
      theme: ThemeData(
        colorScheme: ColorScheme.fromSeed(seedColor: const Color(0xff2f6f73)),
        useMaterial3: true,
      ),
      routerConfig: appRouter,
    );
  }
}
