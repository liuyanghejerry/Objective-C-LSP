/// Protocol fixture for testing protocol symbol extraction.
@protocol Greetable <NSObject>

- (NSString *)greet;

@optional
- (NSString *)farewell;

@end

@interface Robot : NSObject <Greetable>

@property (nonatomic, copy) NSString *model;

- (instancetype)initWithModel:(NSString *)model;

@end
