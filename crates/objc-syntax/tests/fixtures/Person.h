/// A sample Objective-C header used by multiple integration tests.
@interface Person : NSObject

/// The person's full name.
@property (nonatomic, copy) NSString *name;

/// The person's age in years.
@property (nonatomic, assign) NSInteger age;

/// Designated initializer.
- (instancetype)initWithName:(NSString *)name age:(NSInteger)age;

/// Returns a greeting string.
- (NSString *)greet;

/// Class factory method.
+ (instancetype)personWithName:(NSString *)name age:(NSInteger)age;

@end
